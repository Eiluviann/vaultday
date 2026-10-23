import {
	mkdirSync,
	readFileSync,
	realpathSync,
	symlinkSync,
	writeFileSync,
} from 'node:fs';
import path from 'node:path';
import { describe, expect, it } from 'vitest';
import { logicalWorkingDirectory } from '../src/core.ts';
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

describe('the working directory', () => {
	it('is the shell’s logical one when it names the same folder', () => {
		const { link, target } = linkedFolder(copyMockVault());
		expect(logicalWorkingDirectory(target, shellIn(link))).toBe(link);
	});

	it('is the real one when the shell names another folder or none', () => {
		const vault = copyMockVault();
		const { target } = linkedFolder(vault);
		expect(logicalWorkingDirectory(target, shellIn(vault))).toBe(target);
		expect(logicalWorkingDirectory(target, {})).toBe(target);
		expect(
			logicalWorkingDirectory(target, shellIn(path.join(vault, 'gone'))),
		).toBe(target);
	});
});

describe('workspace resolution', () => {
	it('takes a linked folder as part of the root’s workspace', async () => {
		const vault = copyMockVault();
		const { link } = linkedFolder(vault);
		const run = await runVaultday(
			['today', '--json', '--date', '2026-10-22'],
			link,
		);
		expect(run.code).toBe(0);
		expect(JSON.parse(run.out).workspaceName).toBe('Mock Studio');
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
