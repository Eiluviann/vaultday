import { describe, expect, it } from 'vitest';
import { guardVaultPath, parseCliOptions } from '../src/cli-options.ts';
import { copyFixtures } from './helpers.ts';

describe('cli options', () => {
	it('parses rules, mode and paths', () => {
		const result = parseCliOptions(
			['--vault', '/tmp/x', '--dry-run', '--rules', 'frontmatter,tasks'],
			'/pkg',
		);
		expect(result.ok).toBe(true);
		if (!result.ok) return;
		expect(result.value.mode).toBe('dry-run');
		expect([...result.value.rules]).toEqual(['frontmatter', 'tasks']);
		expect(result.value.configDirectory).toBe('/pkg/config');
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
		const base = parseCliOptions(['--vault', workspace.work, '--apply'], '/pkg');
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
