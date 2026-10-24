import { mkdirSync, symlinkSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { InMemoryTransport } from '@modelcontextprotocol/sdk/inMemory.js';
import { describe, expect, it } from 'vitest';
import { createVaultdayServer } from '../src/mcp-server.ts';
import { copyMockVault, runVaultday, snapshot } from './support.ts';

const DATE = '2026-10-22';
const OUTSIDE_STORY = `---
type: story
number: OTHER-1
status: ready
note_system: "1.0"
---
# Salary review

## Next Steps
- [ ] Compare the offers  [id:: task-outside] [created:: 2026-10-19]
`;

type PlantedVault = {
	/** The workspace a synced copy of arrived in, holding the planted link. */
	readonly vault: string;
	/** The folder beside it the link points at, outside every granted folder. */
	readonly outside: string;
};

/**
 * A synced vault that arrived with a relative symlink `Shared -> ../Private`
 * in its root, pointing at a folder of the user's that no workspace holds.
 */
function plantedVault(): PlantedVault {
	const vault = copyMockVault();
	const outside = path.join(path.dirname(vault), 'Private');
	mkdirSync(path.join(outside, 'OTHER-1 Salary review'), { recursive: true });
	writeFileSync(
		path.join(outside, 'OTHER-1 Salary review', 'OTHER-1 Salary review.md'),
		OUTSIDE_STORY,
	);
	symlinkSync(path.join('..', 'Private'), path.join(vault, 'Shared'));
	return { vault, outside };
}

async function connect(workingDirectory: string): Promise<Client> {
	const [clientSide, serverSide] = InMemoryTransport.createLinkedPair();
	await createVaultdayServer({ workingDirectory, environment: {} }).connect(
		serverSide,
	);
	const client = new Client({ name: 'test', version: '0.0.0' });
	await client.connect(clientSide);
	return client;
}

function textOf(result: Awaited<ReturnType<Client['callTool']>>): string {
	const [first] = Array.isArray(result.content) ? result.content : [];
	return first?.type === 'text' ? first.text : '';
}

describe('a symlink planted in a synced vault', () => {
	it('never shows the CLI a note outside the workspace', async () => {
		const { vault } = plantedVault();
		const run = await runVaultday(['today', '--json', '--date', DATE], vault);
		expect(run.code).toBe(0);
		expect(run.out).toContain('WEB-142');
		expect(run.out).not.toContain('OTHER-1');
		expect(run.out).not.toContain('Shared/');
	});

	it('never lets the CLI write outside the workspace', async () => {
		const { vault, outside } = plantedVault();
		const before = snapshot(outside);
		const added = await runVaultday(
			['task', 'add', 'Leak the numbers', '--story', 'OTHER-1', '--json'],
			vault,
		);
		const done = await runVaultday(['task', 'done', 'task-outside'], vault);
		expect(added.code).not.toBe(0);
		expect(done.code).not.toBe(0);
		expect(snapshot(outside)).toEqual(before);
	});

	it('never reaches the MCP tools', async () => {
		const { vault, outside } = plantedVault();
		const before = snapshot(outside);
		const client = await connect(vault);
		const today = await client.callTool({
			name: 'today',
			arguments: { date: DATE },
		});
		expect(textOf(today)).toContain('WEB-142');
		expect(textOf(today)).not.toContain('OTHER-1');
		const added = await client.callTool({
			name: 'task_add',
			arguments: { text: 'Leak the numbers', story: 'OTHER-1' },
		});
		const done = await client.callTool({
			name: 'task_done',
			arguments: { id: 'task-outside' },
		});
		expect(added.isError).toBe(true);
		expect(done.isError).toBe(true);
		expect(snapshot(outside)).toEqual(before);
	});
});
