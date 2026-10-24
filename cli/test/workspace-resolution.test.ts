import {
	mkdirSync,
	readFileSync,
	realpathSync,
	symlinkSync,
	writeFileSync,
} from 'node:fs';
import path from 'node:path';
import { describe, expect, it } from 'vitest';
import { copyMockVault, emptyDirectory, runVaultday } from './support.ts';

/** The environment of a shell whose logical directory is `directory`. */
function shellIn(directory: string): Record<string, string> {
	return Object.fromEntries([['PWD', directory]]);
}

/** A folder outside the vault, linked into its root as `Shared`. */
function linkedFolder(vault: string): { link: string; target: string } {
	const target = path.join(realpathSync(emptyDirectory()), 'Shared');
	mkdirSync(target);
	const link = path.join(vault, 'Shared');
	symlinkSync(target, link);
	return { link, target };
}

describe('workspace resolution', () => {
	it('does not take a linked folder into the root’s workspace, even through the shell’s PWD', async () => {
		const vault = copyMockVault();
		const { link, target } = linkedFolder(vault);
		const run = await runVaultday(['today'], target, shellIn(link));
		expect(run.code).not.toBe(0);
		expect(run.error).toContain('is not a workspace');
	});

	it('keeps a folder’s own workspace when a link elsewhere leads to it', async () => {
		const client = copyMockVault();
		const home = path.join(realpathSync(emptyDirectory()), 'Home');
		mkdirSync(path.join(home, '.vaultday'), { recursive: true });
		writeFileSync(
			path.join(home, '.vaultday', 'settings.json'),
			'{"schema_version": 1, "name": "Home", "kind": "life"}',
		);
		const projects = path.join(realpathSync(client), 'Projects');
		symlinkSync(projects, path.join(home, 'Client projects'));
		const run = await runVaultday(
			['today', '--json', '--date', '2026-10-22'],
			projects,
			shellIn(path.join(home, 'Client projects')),
		);
		expect(run.code).toBe(0);
		expect(JSON.parse(run.out).workspaceName).toBe('Mock Studio');
	});

	it('does not take a link outside every root as part of a workspace', async () => {
		const { target } = linkedFolder(copyMockVault());
		const elsewhere = path.join(realpathSync(emptyDirectory()), 'Shortcut');
		symlinkSync(target, elsewhere);
		const run = await runVaultday(['today'], target, shellIn(elsewhere));
		expect(run.code).not.toBe(0);
	});

	it('refuses an inactive workspace and says how to bring it back', async () => {
		const vault = copyMockVault();
		const settingsPath = path.join(vault, '.vaultday', 'settings.json');
		const settings: unknown = JSON.parse(readFileSync(settingsPath, 'utf8'));
		writeFileSync(
			settingsPath,
			JSON.stringify(Object.assign({}, settings, { active: false })),
		);
		const run = await runVaultday(['today', '--date', '2026-10-22'], vault);
		expect(run.code).not.toBe(0);
		expect(run.error).toContain(
			'Mock Studio is inactive; reactivate it in Vaultday',
		);
	});
});
