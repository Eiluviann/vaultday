import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { parseArguments } from '../src/arguments.ts';
import { loadToday, renderToday } from '../src/today.ts';

const repoRoot = path.resolve(
	path.dirname(fileURLToPath(import.meta.url)),
	'..',
	'..',
);
const mockVault = path.join(repoRoot, 'core', 'fixtures', 'mock-vault');

describe('loadToday', () => {
	it('lists the stories with open work and the day log of the mock vault', () => {
		const result = loadToday({ workspacePath: mockVault, date: '2026-10-22' });
		expect(result.ok).toBe(true);
		if (!result.ok) return;
		const { view } = result.value;
		expect(view.workspaceName).toBe('Mock Studio');
		expect(view.stories.map((story) => story.number)).toEqual([
			'WEB-142',
			'WEB-157',
			'ACME-9',
		]);
		expect(view.stories[0]?.nextSteps[2]?.state).toBe('in-progress');
		expect(view.stories[0]?.nextSteps[2]?.depth).toBe(1);
		expect(
			view.stories[0]?.pendingInput.map((task) => task.assignedTo),
		).toEqual(['Dana']);
		expect(view.log.map((line) => line.time)).toEqual([
			'08:45',
			'09:30',
			'11:15',
			'14:00',
		]);
		expect(view.log[0]?.story).toBe('WEB-142');
	});

	it('defaults to the local day', () => {
		const result = loadToday({ workspacePath: mockVault });
		expect(result.ok).toBe(true);
		if (!result.ok) return;
		expect(result.value.view.date).toMatch(/^\d{4}-\d{2}-\d{2}$/);
	});

	it('fails with the core message for a folder without a manifest', () => {
		const result = loadToday({ workspacePath: repoRoot });
		expect(result.ok).toBe(false);
		if (result.ok) return;
		expect(result.error.message).toContain('not a workspace');
	});

	it('fails for a malformed date', () => {
		const result = loadToday({ workspacePath: mockVault, date: 'yesterday' });
		expect(result.ok).toBe(false);
		if (result.ok) return;
		expect(result.error.message).toContain('YYYY-MM-DD');
	});
});

describe('renderToday', () => {
	it('prints stories, tasks with their depth and the log', () => {
		const result = loadToday({ workspacePath: mockVault, date: '2026-10-22' });
		if (!result.ok) throw result.error;
		const text = renderToday(result.value.view);
		expect(text).toContain('Mock Studio · Today 2026-10-22');
		expect(text).toContain('## WEB-142 · Checkout redesign');
		expect(text).toContain('\t- [/] Handle the declined-card response  (45m)');
		expect(text).toContain(
			'- [ ] Final copy for the order summary  (waiting on Dana)',
		);
		expect(text).toContain(
			'- 08:45 Address form extracted and reviewed [done]',
		);
	});
});

describe('parseArguments', () => {
	it('reads the today command with its flags', () => {
		expect(
			parseArguments(['today', '/v', '--date', '2026-10-22', '--timing']),
		).toEqual({
			command: 'today',
			workspacePath: '/v',
			date: '2026-10-22',
			shouldPrintTiming: true,
		});
	});

	it('falls back to help for anything else', () => {
		expect(parseArguments([])).toEqual({ command: 'help' });
		expect(parseArguments(['today'])).toEqual({ command: 'help' });
		expect(parseArguments(['today', '/v', '--bogus'])).toEqual({
			command: 'help',
		});
		expect(parseArguments(['today', '/v', '--date'])).toEqual({
			command: 'help',
		});
		expect(parseArguments(['today', '/v', '--date', '--timing'])).toEqual({
			command: 'help',
		});
	});
});
