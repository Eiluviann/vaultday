import { describe, expect, it } from 'vitest';
import { parseArguments } from '../src/arguments.ts';

describe('parseArguments', () => {
	it('reads each command with its flags', () => {
		expect(
			parseArguments(['today', '/v', '--date', '2026-10-22', '--json']),
		).toEqual({
			ok: true,
			value: {
				name: 'today',
				startDirectory: '/v',
				date: '2026-10-22',
				shouldPrintJson: true,
			},
		});
		expect(
			parseArguments([
				'log',
				'decision',
				'Use',
				'a',
				'form',
				'--story',
				'WEB-142',
				'--ai',
			]),
		).toMatchObject({
			ok: true,
			value: {
				name: 'log',
				kind: 'decision',
				text: 'Use a form',
				story: 'WEB-142',
				isModelText: true,
			},
		});
		expect(parseArguments(['task', 'done', 'task-5'])).toMatchObject({
			ok: true,
			value: { name: 'task-done', id: 'task-5' },
		});
		expect(parseArguments(['mcp'])).toEqual({
			ok: true,
			value: { name: 'mcp' },
		});
	});

	it('defaults a task to Next Steps', () => {
		expect(
			parseArguments(['task', 'add', 'x', '--story', 'A-1']),
		).toMatchObject({
			ok: true,
			value: { name: 'task-add', section: 'next' },
		});
	});

	it('refuses what it does not understand', () => {
		for (const argv of [
			['frobnicate'],
			['log', 'party', 'x'],
			['log', 'note'],
			['today', '--bogus'],
			['task', 'add', 'x', '--story', 'A-1', '--section', 'later'],
			['task', 'done'],
			['report', 'WEB-142', '--from', '2026-10-21'],
			['log', 'note', 'x', '--ai', '--human'],
		]) {
			const parsed = parseArguments(argv);
			expect(parsed.ok, argv.join(' ')).toBe(false);
		}
	});

	it('asks for help without a command', () => {
		expect(parseArguments([])).toEqual({
			ok: true,
			value: { name: 'help', isRequested: false },
		});
		expect(parseArguments(['--help'])).toEqual({
			ok: true,
			value: { name: 'help', isRequested: true },
		});
	});
});
