import {
	existsSync,
	mkdirSync,
	mkdtempSync,
	readdirSync,
	readFileSync,
	rmSync,
	statSync,
	writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { openWorkspace, reportBundleJson, today } from '@vaultday/core-node';
import { reportNoteText } from '@vaultday/provenance-core';
import { afterEach, describe, expect, it } from 'vitest';
import { planCalendar } from '../src/calendar.ts';
import { LAUNCHER_NAME } from '../src/launcher.ts';
import { buildPlaygroundPlan } from '../src/playground.ts';
import { PLAYGROUND_MARKER, writePlayground } from '../src/write-playground.ts';

const FIXTURES_ROOT = path.resolve(
	path.dirname(fileURLToPath(import.meta.url)),
	'../../../core/fixtures',
);
const temporaryFolders: string[] = [];

afterEach(() => {
	for (const folder of temporaryFolders.splice(0))
		rmSync(folder, { recursive: true, force: true });
});

describe('the generated playground', () => {
	it('gives the core a work week with sessions, billing and waiting-on items', () => {
		const outDirectory = generate('2026-10-24', '11:00');
		const workspace = openWorkspace(path.join(outDirectory, 'Mock Studio'));
		const day = today(workspace, '2026-10-23');
		expect(day.workspaceName).toBe('Mock Studio');
		expect(day.stories.map((story) => story.number)).toEqual(
			expect.arrayContaining([
				'WEB-142',
				'WEB-163',
				'ACME-15',
				'WEB-171',
				'ACME-18',
			]),
		);
		expect(day.waitingOn.map((waiting) => waiting.assignedTo)).toEqual(
			expect.arrayContaining([
				'Dana',
				'Lee',
				'Morgan',
				'Priya',
				'Robin',
				'Sam',
			]),
		);
		const bundle: unknown = JSON.parse(
			reportBundleJson(workspace, 'Mock Studio', '2026-10-19', '2026-10-23'),
		);
		expect(bundle).toMatchObject({
			billing: 'hourly',
			sessions: { count: 23, unmeasuredCount: 0, loggedMinutes: 1835 },
			hours: { model: 'hourly', roundingMinutes: 15 },
		});
		expect(
			readdirSync(path.join(outDirectory, 'Mock Studio', 'Log')),
		).toHaveLength(9);
	});

	it('gives the core a life workspace with today’s log and the leave note', () => {
		const outDirectory = generate('2026-10-24', '11:00');
		const day = today(
			openWorkspace(path.join(outDirectory, 'Personal')),
			'2026-10-24',
		);
		expect(day.workspaceName).toBe('Personal');
		expect(day.log.map((line) => line.time)).toEqual(['08:10', '10:45']);
		expect(day.waitingOn.map((waiting) => waiting.assignedTo)).toEqual([
			'Alex',
			'Jordan',
		]);
		expect(
			readFileSync(path.join(outDirectory, 'Personal', 'Leave.md'), 'utf8'),
		).toContain('- 2026-10-16 Long weekend');
	});

	it('holds a Monday-morning run to its moment: no line, note, marker or completion later', () => {
		const outDirectory = generate('2026-10-26', '09:30');
		const root = path.join(outDirectory, 'Mock Studio');
		expect(readdirSync(path.join(root, 'Log')).sort()).toEqual([
			'2026-10-19.md',
			'2026-10-20.md',
			'2026-10-21.md',
			'2026-10-22.md',
			'2026-10-26.md',
		]);
		const todayTimes = [
			...readFileSync(path.join(root, 'Log', '2026-10-26.md'), 'utf8').matchAll(
				/^- (\d{2}:\d{2}) /gm,
			),
		].map((match) => match[1] ?? '');
		expect(todayTimes).toEqual(['09:00']);
		const notes = listMarkdown(root)
			.filter((relativePath) => !relativePath.startsWith('Log/'))
			.map((relativePath) =>
				readFileSync(path.join(root, relativePath), 'utf8'),
			);
		const datedPatterns = [
			/\[(?:completion|created):: (\d{4}-\d{2}-\d{2})\]/g,
			/^\s*- (\d{4}-\d{2}-\d{2}) /gm,
			/%% ai:(?:note|begin) \S+ · (\d{4}-\d{2}-\d{2}) ·/g,
		];
		const laterDates = notes
			.flatMap((text) =>
				datedPatterns.flatMap((pattern) =>
					[...text.matchAll(pattern)].map((match) => match[1] ?? ''),
				),
			)
			.filter((date) => date > '2026-10-26');
		expect(laterDates).toEqual([]);
		expect(
			readFileSync(
				path.join(
					root,
					'Projects/Website/WEB-163 Product page performance/WEB-163 Product page performance.md',
				),
				'utf8',
			),
		).toContain('- [ ] Profile the product page on a mid-range phone');
	});

	it('leaves every provenance marker intact after the dates moved', () => {
		const root = path.join(generate('2026-10-26', '09:30'), 'Mock Studio');
		const statuses = listMarkdown(root).flatMap((relativePath) =>
			reportNoteText(
				relativePath,
				readFileSync(path.join(root, relativePath), 'utf8'),
			).units.map((unit) => unit.status),
		);
		expect(statuses.length).toBeGreaterThan(0);
		expect(new Set(statuses)).toEqual(new Set(['intact']));
	});

	it('writes an executable launcher that names both roots', () => {
		const launcher = path.join(generate('2026-10-24', '11:00'), LAUNCHER_NAME);
		expect(statSync(launcher).mode & 0o111).toBe(0o111);
		expect(readFileSync(launcher, 'utf8')).toContain(
			'for root in "Mock Studio" "Personal"; do',
		);
	});
});

describe('a reset', () => {
	it('replaces the notes and keeps the app’s and Obsidian’s state', () => {
		const outDirectory = generate('2026-10-24', '11:00');
		const root = path.join(outDirectory, 'Mock Studio');
		mkdirSync(path.join(root, '.vaultday', 'sync'), { recursive: true });
		writeFileSync(
			path.join(root, '.vaultday', 'sync', 'workspace.id'),
			'ws-kept\n',
		);
		writeFileSync(
			path.join(root, '.obsidian', 'app.json'),
			'{"useTab":false}\n',
		);
		writeFileSync(path.join(root, 'Scratch.md'), '# Added while playing\n');
		writeFileSync(path.join(root, 'Home.md'), '# Edited\n');
		const written = writePlayground(
			planFor(outDirectory, '2026-10-24', '11:00'),
		);
		expect(written).toMatchObject({
			ok: true,
			value: { keptAppState: ['Mock Studio'] },
		});
		expect(
			readFileSync(
				path.join(root, '.vaultday', 'sync', 'workspace.id'),
				'utf8',
			),
		).toBe('ws-kept\n');
		expect(readFileSync(path.join(root, '.obsidian', 'app.json'), 'utf8')).toBe(
			'{"useTab":false}\n',
		);
		expect(existsSync(path.join(root, 'Scratch.md'))).toBe(false);
		expect(readFileSync(path.join(root, 'Home.md'), 'utf8')).toContain(
			'type: home',
		);
	});

	it('refuses a root it did not write', () => {
		const outDirectory = temporaryFolder();
		mkdirSync(path.join(outDirectory, 'Personal'));
		writeFileSync(path.join(outDirectory, 'Personal', 'Diary.md'), '# Mine\n');
		expect(
			writePlayground(planFor(outDirectory, '2026-10-24', '11:00')),
		).toMatchObject({ ok: false });
		expect(
			readFileSync(path.join(outDirectory, 'Personal', 'Diary.md'), 'utf8'),
		).toBe('# Mine\n');
		expect(existsSync(path.join(outDirectory, 'Mock Studio'))).toBe(false);
	});

	it('refuses a real workspace given as the playground folder, and leaves it untouched', () => {
		const workspace = temporaryFolder();
		mkdirSync(path.join(workspace, '.vaultday'));
		writeFileSync(
			path.join(workspace, '.vaultday', 'settings.json'),
			'{"schema_version":1,"name":"Work","kind":"work"}\n',
		);
		writeFileSync(path.join(workspace, 'Home.md'), '# Mine\n');
		const nested = path.join(workspace, 'Projects', 'Playground');
		for (const outDirectory of [workspace, nested]) {
			const written = writePlayground(
				planFor(outDirectory, '2026-10-24', '11:00'),
			);
			expect(written).toMatchObject({ ok: false });
			if (!written.ok)
				expect(written.error).toContain(`workspace root ${workspace}`);
		}
		expect(readdirSync(workspace).sort()).toEqual(['.vaultday', 'Home.md']);
		expect(readFileSync(path.join(workspace, 'Home.md'), 'utf8')).toBe(
			'# Mine\n',
		);
	});

	it('refuses a root that is a file', () => {
		const outDirectory = temporaryFolder();
		writeFileSync(path.join(outDirectory, 'Mock Studio'), 'not a folder\n');
		const written = writePlayground(
			planFor(outDirectory, '2026-10-24', '11:00'),
		);
		expect(written).toMatchObject({ ok: false });
		if (!written.ok) expect(written.error).toContain('is a file');
	});

	it('marks each root, so a later run may reset it', () => {
		const outDirectory = generate('2026-10-24', '11:00');
		expect(
			existsSync(path.join(outDirectory, 'Personal', PLAYGROUND_MARKER)),
		).toBe(true);
		expect(
			writePlayground(planFor(outDirectory, '2026-10-24', '11:00')),
		).toMatchObject({ ok: true });
	});
});

function generate(todayDate: string, now: string): string {
	const outDirectory = temporaryFolder();
	const written = writePlayground(planFor(outDirectory, todayDate, now));
	if (!written.ok) throw new Error(written.error);
	return outDirectory;
}

function planFor(outDirectory: string, todayDate: string, now: string) {
	return buildPlaygroundPlan({
		outDirectory,
		calendar: planCalendar(todayDate, now),
		fixturesRoot: FIXTURES_ROOT,
	});
}

function temporaryFolder(): string {
	const folder = mkdtempSync(path.join(tmpdir(), 'vaultday-playground-'));
	temporaryFolders.push(folder);
	return folder;
}

function listMarkdown(root: string, relativeFolder = ''): string[] {
	return readdirSync(path.join(root, relativeFolder), {
		withFileTypes: true,
	}).flatMap((entry) => {
		const relativePath =
			relativeFolder === '' ? entry.name : `${relativeFolder}/${entry.name}`;
		if (entry.isDirectory())
			return entry.name.startsWith('.') ? [] : listMarkdown(root, relativePath);
		return entry.name.endsWith('.md') ? [relativePath] : [];
	});
}
