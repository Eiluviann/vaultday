import { beforeAll, describe, expect, it } from 'vitest';
import { refreshFingerprints } from '../src/fingerprint-refresh.ts';
import {
	insertLines,
	openDocument,
	replaceLineText,
} from '../src/note-document.ts';
import type { ProvenanceHasher } from '../src/provenance-core.ts';
import { referenceHasher } from '../src/provenance-core.ts';
import { scanUnits } from '../src/provenance-units.ts';
import {
	CURRENT_WIDGET,
	changeFor,
	copyFixtures,
	type FixtureWorkspace,
	HUB,
	loadHasher,
	planFixture,
	WIDGET_STORY,
} from './helpers.ts';

let hasher: ProvenanceHasher;
let workspace: FixtureWorkspace;

beforeAll(async () => {
	hasher = await loadHasher();
	workspace = copyFixtures();
});

const MARKER_LINE = /^(?:> )*%% ai:/;

function markerLines(text: string): string[] {
	return text.split('\n').filter((line) => MARKER_LINE.test(line));
}

describe('provenance markers', () => {
	it('marker lines stay byte-identical except for refreshed hashes; the verified token survives', () => {
		const { plan } = planFixture(workspace.work, 'mini-work', workspace, hasher);
		for (const change of plan.changes) {
			const before = markerLines(change.before);
			const after = markerLines(change.after);
			expect(after.length).toBe(before.length);
			before.forEach((line, index) => {
				const expected = line.replace(
					/ · [0-9a-f]{12}( · verified| %%)/,
					' · HASH$1',
				);
				const actual = (after[index] ?? '').replace(
					/ · [0-9a-f]{12}( · verified| %%)/,
					' · HASH$1',
				);
				expect(actual).toBe(expected);
			});
		}
		const hub = changeFor(plan, HUB).after;
		expect(hub).toMatch(
			/%% ai:begin claude-test-1 · 2026-10-01 · [0-9a-f]{12} · verified 2026-10-21 · df02faf73c14 %%/,
		);
		expect(hub).not.toContain('· df02faf73c14 · verified');
	});

	it('refreshed hashes equal the hook core hash of the new unit body', () => {
		const { plan } = planFixture(workspace.work, 'mini-work', workspace, hasher);
		for (const change of plan.changes) {
			const lines = change.after.split('\n');
			for (const unit of scanUnits(lines)) {
				if (unit.hash === undefined) continue;
				const body = lines.slice(unit.bodyStart, unit.bodyEnd + 1);
				expect(hasher.hashText(hasher.normalizeLines(body, unit.prefix))).toBe(
					unit.hash,
				);
			}
		}
	});

	it('id insertion, field spacing and link retargeting inside units refresh the fingerprint', () => {
		const { plan } = planFixture(workspace.work, 'mini-work', workspace, hasher);
		const kinds = Object.fromEntries(
			plan.changes.flatMap((change) =>
				change.refreshes.map((refresh) => [
					`${change.fromPath}:${refresh.markerLine}`,
					refresh,
				]),
			),
		);
		expect(kinds[`${HUB}:17`]).toMatchObject({
			result: 'refreshed',
			edits: ['task'],
		});
		expect(kinds[`${CURRENT_WIDGET}:9`]).toMatchObject({
			result: 'refreshed',
			edits: ['link'],
		});
		expect(kinds[`${WIDGET_STORY}:29`]).toMatchObject({
			result: 'refreshed',
			edits: ['task'],
		});
		expect(
			plan.changes
				.flatMap((change) => change.refreshes)
				.every((refresh) => refresh.result === 'refreshed'),
		).toBe(true);
	});

	it('a blockquoted block keeps its prefix and is hashed without it', () => {
		const { plan } = planFixture(workspace.work, 'mini-work', workspace, hasher);
		const after = changeFor(plan, 'Test users.md').after;
		expect(after).toMatch(
			/^> %% ai:begin claude-test-1 · 2026-10-01 · [0-9a-f]{12} %%$/m,
		);
		expect(after).toMatch(
			/^> - \[ \] Check the basic user still exists {2}\[id:: task-\d+\] \[assigned_to:: me\]$/m,
		);
	});

	it('a structural change inside a unit leaves it revised instead of refreshing it', () => {
		const text = [
			'%% ai:begin m · 2026-01-01 · HASH %%',
			'line one',
			'%% ai:end %%',
		].join('\n');
		const hash = referenceHasher.hashText(
			referenceHasher.normalizeLines(['line one']),
		);
		const document = openDocument('x.md', text.replace('HASH', hash));
		insertLines(document, 2, ['## inserted'], 'structure');
		const outcomes = refreshFingerprints(document, referenceHasher);
		expect(outcomes).toEqual([
			{
				markerLine: 1,
				kind: 'block',
				result: 'revised',
				edits: ['structure'],
				oldHash: hash,
			},
		]);
		expect(document.lines[0]?.text).toBe(
			`%% ai:begin m · 2026-01-01 · ${hash} %%`,
		);
	});

	it('a unit that was already revised before migration is not re-stamped', () => {
		const document = openDocument(
			'x.md',
			['%% ai:note m · 2026-01-01 · 000000000000 %%', '- [ ] task [k::v]'].join(
				'\n',
			),
		);
		replaceLineText(document, 1, '- [ ] task [id:: task-1] [k:: v]', 'task');
		expect(refreshFingerprints(document, referenceHasher)[0]?.result).toBe(
			'already-revised',
		);
		expect(document.lines[0]?.text).toContain('000000000000');
	});

	it('the reference hasher matches the hook core', () => {
		const lines = ['> first  ', '> ', '>   second', ''];
		expect(referenceHasher.normalizeLines(lines, '> ')).toBe(
			hasher.normalizeLines(lines, '> '),
		);
		expect(referenceHasher.hashText('a\nb')).toBe(hasher.hashText('a\nb'));
	});
});
