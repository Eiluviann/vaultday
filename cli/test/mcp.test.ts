import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { InMemoryTransport } from '@modelcontextprotocol/sdk/inMemory.js';
import { describe, expect, it } from 'vitest';
import { createVaultdayServer } from '../src/mcp-server.ts';
import { copyMockVault, emptyDirectory, modelSession } from './support.ts';

async function connect(
	workingDirectory: string,
	environment = {},
): Promise<Client> {
	const [clientSide, serverSide] = InMemoryTransport.createLinkedPair();
	await createVaultdayServer({ workingDirectory, environment }).connect(
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

describe('the MCP server', () => {
	it('lists the commands as tools with JSON schemas', async () => {
		const client = await connect(copyMockVault());
		const { tools } = await client.listTools();
		expect(tools.map((tool) => tool.name).sort()).toEqual([
			'log',
			'report',
			'status',
			'task_add',
			'task_done',
			'timer_continue',
			'timer_start',
			'timer_stop',
			'today',
		]);
		const log = tools.find((tool) => tool.name === 'log');
		expect(log?.inputSchema.type).toBe('object');
		expect(log?.inputSchema.required).toEqual(['kind', 'text']);
		expect(JSON.stringify(log?.inputSchema.properties)).toContain('decision');
	});

	it('answers today, logs a model line with its author and reports it back', async () => {
		const vault = copyMockVault();
		const client = await connect(vault, modelSession('claude-fable-5-1'));
		const today = await client.callTool({
			name: 'today',
			arguments: { date: '2026-10-22' },
		});
		expect(textOf(today)).toContain('## WEB-142 · Checkout redesign');

		const logged = await client.callTool({
			name: 'log',
			arguments: {
				kind: 'decision',
				text: 'Keep the coupon field',
				story: 'WEB-142',
			},
		});
		expect(logged.isError).toBeUndefined();
		expect(textOf(logged)).toContain('[agent:: claude-fable-5-1]');

		const fact = await client.callTool({
			name: 'log',
			arguments: {
				kind: 'note',
				text: 'Dictated by the user',
				isModelText: false,
			},
		});
		expect(textOf(fact)).not.toContain('agent');

		const done = await client.callTool({
			name: 'task_done',
			arguments: { id: 'task-6' },
		});
		expect(textOf(done)).toContain('[x] Final copy');
	});

	it('answers an error outside a workspace', async () => {
		const client = await connect(emptyDirectory());
		const result = await client.callTool({ name: 'today', arguments: {} });
		expect(result.isError).toBe(true);
		expect(textOf(result)).toContain('is not a workspace');
	});

	it('rejects arguments that break the schema', async () => {
		const client = await connect(copyMockVault());
		const result = await client
			.callTool({ name: 'log', arguments: { kind: 'party', text: 'x' } })
			.catch((error: unknown) => error);
		const failed =
			result instanceof Error ||
			(typeof result === 'object' &&
				result !== null &&
				'isError' in result &&
				result.isError === true);
		expect(failed).toBe(true);
	});
});
