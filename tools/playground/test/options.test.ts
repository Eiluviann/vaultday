import {
	mkdirSync,
	mkdtempSync,
	realpathSync,
	rmSync,
	symlinkSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { type OptionsEnvironment, parseOptions } from '../src/options.ts';

let home = '';
let environment: OptionsEnvironment;

beforeEach(() => {
	home = realpathSync(
		mkdtempSync(path.join(tmpdir(), 'vaultday-playground-home-')),
	);
	mkdirSync(path.join(home, 'Vaults', 'Work'), { recursive: true });
	mkdirSync(path.join(home, 'Library'));
	mkdirSync(path.join(home, 'vaultday', 'core', 'fixtures', 'mock-vault'), {
		recursive: true,
	});
	environment = {
		homeDirectory: home,
		repositoryRoots: [path.join(home, 'vaultday')],
		clock: new Date(2026, 9, 24, 9, 5),
	};
});

afterEach(() => {
	rmSync(home, { recursive: true, force: true });
});

describe('parseOptions', () => {
	it('defaults to ~/Vaultday-playground and the clock', () => {
		expect(parseOptions([], environment)).toEqual({
			ok: true,
			value: {
				outDirectory: path.join(home, 'Vaultday-playground'),
				today: '2026-10-24',
				now: '09:05',
			},
		});
	});

	it('takes the pnpm separator, a folder from home and a pinned date and time', () => {
		expect(
			parseOptions(
				['--', '--out', '~/pg', '--today', '2026-10-26', '--now', '13:30'],
				environment,
			),
		).toEqual({
			ok: true,
			value: {
				outDirectory: path.join(home, 'pg'),
				today: '2026-10-26',
				now: '13:30',
			},
		});
	});

	it('refuses an impossible date or time, an unknown flag and a positional', () => {
		for (const argv of [
			['--today', '2026-02-30'],
			['--now', '7:00'],
			['--force'],
			['somewhere'],
		]) {
			expect(parseOptions(argv, environment)).toMatchObject({ ok: false });
		}
	});

	it('refuses the vaults, the Library and a checkout, whatever the case or a link says', () => {
		symlinkSync(path.join(home, 'Vaults'), path.join(home, 'shortcut'));
		const outs = [
			'~/Vaults/Work',
			'~/vaults/Work',
			'~/VAULTS/New folder',
			'~/shortcut/Work',
			'~/Library/Group Containers/x',
			'~/vaultday/core/fixtures/mock-vault',
		];
		for (const out of outs) {
			const parsed = parseOptions(['--out', out], environment);
			expect(parsed, out).toMatchObject({ ok: false });
			if (!parsed.ok)
				expect(parsed.error).toContain('the playground never goes there');
		}
	});

	it('accepts a folder beside the protected ones', () => {
		expect(
			parseOptions(['--out', '~/Vaults-playground'], environment),
		).toMatchObject({ ok: true });
	});
});
