import { homedir } from 'node:os';
import path from 'node:path';
import { describe, expect, it } from 'vitest';
import {
	defaultConfigDirectory,
	guardVaultPath,
	parseCliOptions,
} from '../src/cli-options.ts';
import { copyFixtures } from './helpers.ts';

describe('cli options', () => {
	it('parses rules, mode and paths', () => {
		const previousXdgConfigHome = process.env.XDG_CONFIG_HOME;
		delete process.env.XDG_CONFIG_HOME;
		const result = parseCliOptions(
			['--vault', '/tmp/x', '--dry-run', '--rules', 'frontmatter,tasks'],
			'/pkg',
		);
		if (previousXdgConfigHome !== undefined)
			process.env.XDG_CONFIG_HOME = previousXdgConfigHome;
		expect(result.ok).toBe(true);
		if (!result.ok) return;
		expect(result.value.mode).toBe('dry-run');
		expect([...result.value.rules]).toEqual(['frontmatter', 'tasks']);
		expect(result.value.configDirectory).toBe(
			path.join(homedir(), '.config/vault-tools/vault-migrate'),
		);
	});

	it('rejects unknown rules and missing mode', () => {
		expect(
			parseCliOptions(
				['--vault', '/tmp/x', '--dry-run', '--rules', 'nope'],
				'/pkg',
			).ok,
		).toBe(false);
		expect(parseCliOptions(['--vault', '/tmp/x'], '/pkg').ok).toBe(false);
		expect(
			parseCliOptions(['--vault', '/tmp/x', '--dry-run', '--apply'], '/pkg').ok,
		).toBe(false);
	});

	it('refuses iCloud paths, non-vaults and live vaults', () => {
		const workspace = copyFixtures();
		const base = parseCliOptions(
			['--vault', workspace.work, '--apply'],
			'/pkg',
		);
		if (!base.ok) throw new Error(base.error);
		expect(
			guardVaultPath({
				...base.value,
				vaultRoot:
					'/Users/x/Library/Mobile Documents/iCloud~md~obsidian/Documents/Work',
			}).ok,
		).toBe(false);
		expect(
			guardVaultPath({ ...base.value, vaultRoot: workspace.root }).ok,
		).toBe(false);
		expect(guardVaultPath(base.value).ok).toBe(false);
		expect(guardVaultPath({ ...base.value, isLiveAllowed: true }).ok).toBe(
			true,
		);
		expect(guardVaultPath({ ...base.value, mode: 'dry-run' }).ok).toBe(true);
	});
});

describe('defaultConfigDirectory', () => {
	it('prefers XDG_CONFIG_HOME', () => {
		expect(defaultConfigDirectory('/xdg')).toBe(
			'/xdg/vault-tools/vault-migrate',
		);
	});

	it('falls back to ~/.config when XDG_CONFIG_HOME is unset, empty or relative', () => {
		const expected = path.join(homedir(), '.config/vault-tools/vault-migrate');
		expect(defaultConfigDirectory(undefined)).toBe(expected);
		expect(defaultConfigDirectory('')).toBe(expected);
		expect(defaultConfigDirectory('relative/dir')).toBe(expected);
	});
});
