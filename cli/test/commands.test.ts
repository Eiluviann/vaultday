import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { reportNoteText } from '@vaultday/provenance-core';
import { describe, expect, it } from 'vitest';
import {
	copyMockVault,
	diffSnapshots,
	emptyDirectory,
	MOCK_VAULT,
	modelSession,
	runVaultday,
	snapshot,
	WEB_142_NOTE,
} from './support.ts';

const ALL_KINDS = ['blocker', 'decision', 'done', 'note', 'session', 'status'];

describe('the fixture', () => {
	it('is never written to by the tests', async () => {
		const before = snapshot(MOCK_VAULT);
		const vault = copyMockVault();
		await runVaultday(['log', 'note', 'x', '--date', '2026-10-22'], vault);
		expect(diffSnapshots(before, snapshot(MOCK_VAULT))).toEqual([]);
	});
});

describe('vaultday today', () => {
	it('lists the stories with open work, the day log and who is waited on', async () => {
		const vault = copyMockVault();
		const run = await runVaultday(['today', '--date', '2026-10-22'], vault);
		expect(run.code).toBe(0);
		for (const heading of [
			'## WEB-142 · Checkout redesign',
			'## WEB-157 · Newsletter signup',
			'## ACME-9 · Onboarding emails',
		]) {
			expect(run.out).toContain(heading);
		}
		expect(run.out).toContain(
			'\t- [/] Handle the declined-card response  (45m)',
		);
		expect(run.out).toContain(
			'- 08:45 Address form extracted and reviewed [done]',
		);
		expect(run.out).toMatch(
			/## Waiting on\n- Priya: Budget for the photo shoot, 21 days/,
		);
	});

	it('resolves the workspace from a story folder, deepest first', async () => {
		const vault = copyMockVault();
		const storyFolder = path.join(
			vault,
			'Projects/Website/WEB-142 Checkout redesign',
		);
		const run = await runVaultday(
			['today', '--json', '--date', '2026-10-22'],
			storyFolder,
		);
		expect(run.code).toBe(0);
		const view = JSON.parse(run.out);
		expect(view.workspaceName).toBe('Mock Studio');
		expect(
			view.stories.map((story: { number: string }) => story.number),
		).toEqual(['WEB-142', 'WEB-157', 'ACME-9']);
	});
});

describe('vaultday log', () => {
	it('appends a decision in the day-log format, byte for byte', async () => {
		const vault = copyMockVault();
		const before = snapshot(vault);
		const run = await runVaultday(
			[
				'log',
				'decision',
				'Use a single-page checkout',
				'--story',
				'WEB-142',
				'--task',
				'task-5',
				'--date',
				'2026-10-22',
				'--time',
				'15:05',
			],
			vault,
		);
		expect(run.code).toBe(0);
		const line =
			'- 15:05 decision · Use a single-page checkout [story:: WEB-142] [kind:: decision] [task:: task-5]';
		const original = before.get('Log/2026-10-22.md') ?? '';
		const lastLogLine = '[task:: task-9]\n';
		expect(snapshot(vault).get('Log/2026-10-22.md')).toBe(
			original.replace(lastLogLine, `${lastLogLine}${line}\n`),
		);
		expect(diffSnapshots(before, snapshot(vault))).toEqual([
			{ path: 'Log/2026-10-22.md', removed: [], added: [line] },
		]);
	});

	it('creates the day log with the convention header for a new day', async () => {
		const vault = copyMockVault();
		await runVaultday(
			[
				'log',
				'status',
				'Planned the day',
				'--date',
				'2026-10-23',
				'--time',
				'09:00',
			],
			vault,
		);
		expect(readFileSync(path.join(vault, 'Log/2026-10-23.md'), 'utf8')).toBe(
			'---\ntype: log\ndate: 2026-10-23\n---\n# 2026-10-23\n\n## Log\n- 09:00 status · Planned the day [kind:: status]\n',
		);
	});

	it('leaves fact lines unmarked and attributes model lines to their author', async () => {
		const vault = copyMockVault();
		const fact = await runVaultday(
			[
				'log',
				'done',
				'Shipped the form',
				'--date',
				'2026-10-23',
				'--time',
				'09:00',
			],
			vault,
		);
		expect(fact.out).not.toContain('agent');
		const model = await runVaultday(
			[
				'log',
				'note',
				'Read the gateway docs',
				'--ai',
				'--date',
				'2026-10-23',
				'--time',
				'09:01',
			],
			vault,
			modelSession('claude-fable-5-1'),
		);
		expect(model.out).toContain(
			'- 09:01 note · Read the gateway docs [kind:: note] [agent:: claude-fable-5-1]',
		);
		const sessionEnvironment = await runVaultday(
			[
				'log',
				'note',
				'From the environment',
				'--date',
				'2026-10-23',
				'--time',
				'09:02',
			],
			vault,
			modelSession('claude fable·5'),
		);
		expect(sessionEnvironment.out).toContain('[agent:: claude-fable-5]');
		const human = await runVaultday(
			[
				'log',
				'note',
				'Typed by me',
				'--human',
				'--date',
				'2026-10-23',
				'--time',
				'09:03',
			],
			vault,
			modelSession('claude-fable-5-1'),
		);
		expect(human.out).not.toContain('agent');
	});

	it('refuses a bad kind, an unknown story and a malformed time without writing', async () => {
		const vault = copyMockVault();
		const before = snapshot(vault);
		const badKind = await runVaultday(['log', 'party', 'x'], vault);
		expect(badKind.code).toBe(2);
		const unknownStory = await runVaultday(
			['log', 'note', 'x', '--story', 'NOPE-1'],
			vault,
		);
		expect(unknownStory.code).toBe(1);
		expect(unknownStory.error).toContain('no story numbered NOPE-1');
		const badTime = await runVaultday(
			['log', 'note', 'x', '--time', '25:99'],
			vault,
		);
		expect(badTime.code).toBe(1);
		expect(badTime.error).toContain('HH:MM');
		expect(diffSnapshots(before, snapshot(vault))).toEqual([]);
	});
});

describe('vaultday report', () => {
	it('returns every event kind present in the fixture for a client', async () => {
		const vault = copyMockVault();
		const run = await runVaultday(
			['report', 'Website', '--from', '2026-10-21', '--to', '2026-10-22'],
			vault,
		);
		expect(run.code).toBe(0);
		const bundle = JSON.parse(run.out);
		expect(bundle.subjectKind).toBe('client');
		const kinds = [
			...new Set(bundle.log.map((line: { kind: string }) => line.kind)),
		].sort();
		expect(kinds).toEqual(ALL_KINDS);
		expect(
			bundle.stories.map((story: { number: string }) => story.number),
		).toEqual(['WEB-142', 'WEB-157']);
		const [checkout] = bundle.stories;
		expect(checkout.frontmatter.aliases).toEqual([
			'Checkout v2',
			'Cart redesign',
		]);
		expect(
			checkout.pendingInput.map((task: { status: string }) => task.status),
		).toEqual(['open', 'done']);
		expect(checkout).not.toHaveProperty('noteText');
	});

	it('summarises authorship from the markers and agent fields present', async () => {
		const vault = copyMockVault();
		const run = await runVaultday(
			['report', 'WEB-142', '--from', '2026-10-21', '--to', '2026-10-22'],
			vault,
		);
		const { authorship, subjectKind } = JSON.parse(run.out);
		expect(subjectKind).toBe('story');
		// WEB-142 holds one model-text block.
		expect(authorship.notes).toEqual({ ai: 0, mixed: 1, human: 0 });
		expect(authorship.modelWords).toBeGreaterThan(0);
		expect(authorship.noteReports[0].units[0]).toMatchObject({
			kind: 'block',
			model: 'fable',
			authorKind: 'model',
		});
		const client = JSON.parse(
			(
				await runVaultday(
					['report', 'Website', '--from', '2026-10-21', '--to', '2026-10-22'],
					vault,
				)
			).out,
		);
		expect(client.authorship.logLines).toEqual({
			total: 10,
			byModel: 1,
			byFact: 9,
			models: { 'claude-fable-5-1': 1 },
		});
	});

	it('is the core bundle with only the authorship slot filled in', async () => {
		const vault = copyMockVault();
		const run = await runVaultday(
			['report', 'WEB-142', '--from', '2026-10-21', '--to', '2026-10-22'],
			vault,
		);
		expect(run.code).toBe(0);
		const bundle = JSON.parse(run.out);
		expect(bundle.authorship).not.toBeNull();
		// The order the README documents.
		expect(Object.keys(bundle)).toEqual([
			'subject',
			'subjectKind',
			'from',
			'to',
			'billing',
			'stories',
			'log',
			'sessions',
			'hours',
			'authorship',
		]);
		// The golden is what the core's own test pins; the app shows the same bundle.
		const golden = readFileSync(
			path.join(path.dirname(MOCK_VAULT), 'golden', 'report-web-142.json'),
			'utf8',
		);
		const withoutAuthorship = JSON.stringify(
			{ ...bundle, authorship: null },
			undefined,
			2,
		);
		expect(`${withoutAuthorship}\n`).toBe(golden);
	});

	it('gives an FTE workspace logged hours and never tracked time', async () => {
		const vault = copyMockVault();
		const settingsPath = path.join(vault, '.vaultday', 'settings.json');
		const settings = JSON.parse(readFileSync(settingsPath, 'utf8'));
		writeFileSync(
			settingsPath,
			JSON.stringify({ ...settings, billing: { model: 'fte' } }),
		);
		const run = await runVaultday(
			['report', 'Website', '--from', '2026-10-21', '--to', '2026-10-22'],
			vault,
		);
		expect(run.code).toBe(0);
		const bundle = JSON.parse(run.out);
		expect(bundle.billing).toBe('fte');
		expect(Object.keys(bundle.hours)).toEqual([
			'model',
			'loggedMinutes',
			'byStory',
		]);
		// The mock vault's measured sessions: 1h 30m and 45m.
		expect(bundle.hours.loggedMinutes).toBe(135);
		expect(run.out.toLowerCase()).not.toContain('tracked');
	});

	it('reports unknown subjects and reversed ranges as failures', async () => {
		const vault = copyMockVault();
		expect(
			(
				await runVaultday(
					['report', 'Nobody', '--from', '2026-10-21', '--to', '2026-10-22'],
					vault,
				)
			).code,
		).toBe(1);
		const reversed = await runVaultday(
			['report', 'WEB-142', '--from', '2026-10-22', '--to', '2026-10-21'],
			vault,
		);
		expect(reversed.code).toBe(1);
		expect(reversed.error).toContain('reversed');
		expect((await runVaultday(['report', 'WEB-142'], vault)).code).toBe(2);
	});
});

describe('vaultday task', () => {
	it('done changes exactly one line', async () => {
		const vault = copyMockVault();
		const before = snapshot(vault);
		const run = await runVaultday(['task', 'done', 'task-6'], vault);
		expect(run.code).toBe(0);
		const [change, ...others] = diffSnapshots(before, snapshot(vault));
		expect(others).toEqual([]);
		expect(change?.path).toBe(WEB_142_NOTE);
		expect(change?.removed).toEqual([
			'- [ ] Final copy for the order summary  [id:: task-6] [created:: 2026-10-10] [assigned_to:: Dana]',
		]);
		expect(change?.added).toHaveLength(1);
		expect(change?.added[0]).toMatch(
			/^- \[x\] Final copy for the order summary {2}\[id:: task-6\] \[created:: 2026-10-10\] \[assigned_to:: Dana\] \[completion:: \d{4}-\d{2}-\d{2}\]$/,
		);
	});

	it('done refuses an unknown id', async () => {
		const vault = copyMockVault();
		const before = snapshot(vault);
		const run = await runVaultday(['task', 'done', 'task-999'], vault);
		expect(run.code).toBe(1);
		expect(run.error).toContain('no task with [id:: task-999]');
		expect(diffSnapshots(before, snapshot(vault))).toEqual([]);
	});

	it('add puts a plain line under the chosen section with the next id', async () => {
		const vault = copyMockVault();
		const before = snapshot(vault);
		const run = await runVaultday(
			[
				'task',
				'add',
				'Review the copy',
				'--story',
				'WEB-142',
				'--section',
				'pending',
				'--assigned-to',
				'Dana',
				'--json',
			],
			vault,
		);
		expect(run.code).toBe(0);
		const { id, change } = JSON.parse(run.out);
		expect(id).toBe('task-15');
		expect(change.newLine).toMatch(
			/^- \[ \] Review the copy \[id:: task-15\] \[created:: \d{4}-\d{2}-\d{2}\] \[assigned_to:: Dana\]$/,
		);
		const [fileChange] = diffSnapshots(before, snapshot(vault));
		expect(fileChange?.added).toEqual([change.newLine]);
		expect(fileChange?.removed).toEqual([]);
	});

	it('add wraps model-written text in an intact provenance block', async () => {
		const vault = copyMockVault();
		const run = await runVaultday(
			['task', 'add', 'Draft the release notes', '--story', 'WEB-142', '--ai'],
			vault,
			modelSession('claude-fable-5-1'),
		);
		expect(run.code).toBe(0);
		const text = readFileSync(path.join(vault, WEB_142_NOTE), 'utf8');
		const report = reportNoteText(WEB_142_NOTE, text);
		expect(report.problems).toEqual([]);
		const unit = report.units.find(
			(candidate) => candidate.model === 'claude-fable-5-1',
		);
		expect(unit).toMatchObject({
			kind: 'block',
			authorKind: 'model',
			status: 'intact',
		});
		expect(text).toMatch(
			/\[due:: 2026-10-28\]\n%% ai:begin claude-fable-5-1 · \d{4}-\d{2}-\d{2} · [0-9a-f]{12} %%\n- \[ \] Draft the release notes \[id:: task-15\]/,
		);
		expect(text).toContain('%% ai:end %%\n\n## Pending Input');
	});

	it('add refuses an unknown story and a missing story flag', async () => {
		const vault = copyMockVault();
		expect(
			(await runVaultday(['task', 'add', 'x', '--story', 'NOPE-1'], vault))
				.code,
		).toBe(1);
		expect((await runVaultday(['task', 'add', 'x'], vault)).code).toBe(2);
	});
});

describe('outside a workspace', () => {
	it.each([
		['today'],
		['log', 'note', 'x'],
		['report', 'WEB-142', '--from', '2026-10-21', '--to', '2026-10-22'],
		['task', 'add', 'x', '--story', 'WEB-142'],
		['task', 'done', 'task-1'],
	])('%s refuses with a clear message and a non-zero exit', async (...argv) => {
		const directory = emptyDirectory();
		const run = await runVaultday(argv, directory);
		expect(run.code).toBe(1);
		expect(run.out).toBe('');
		expect(run.error).toContain('is not a workspace');
		expect(run.error).toContain('.vaultday/settings.json');
		expect(snapshot(directory).size).toBe(0);
	});
});

describe('the sync folder', () => {
	it('is left alone and never indexed', async () => {
		const vault = copyMockVault();
		mkdirSync(path.join(vault, '.vaultday/sync/device-a'), { recursive: true });
		writeFileSync(
			path.join(vault, '.vaultday/sync/device-a/0001.md'),
			'---\ntype: story\nnumber: HIDDEN-1\n---\n# Hidden\n\n## Next Steps\n- [ ] secret [id:: task-900]\n',
		);
		const before = snapshot(vault);
		const today = await runVaultday(['today', '--date', '2026-10-22'], vault);
		expect(today.out).not.toContain('HIDDEN-1');
		expect((await runVaultday(['task', 'done', 'task-900'], vault)).code).toBe(
			1,
		);
		expect(
			(
				await runVaultday(
					['report', 'HIDDEN-1', '--from', '2026-10-21', '--to', '2026-10-22'],
					vault,
				)
			).code,
		).toBe(1);
		expect(diffSnapshots(before, snapshot(vault))).toEqual([]);
	});
});
