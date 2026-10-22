import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { describe, it } from 'node:test';
import {
	authorshipOf,
	checkVault,
	formatMarker,
	gateEdit,
	gateWrite,
	hashText,
	listVaultNotes,
	normalizeLines,
	parseMarkers,
	renderIndex,
	resolveVaultNote,
	stampNote,
	UNMARKED_PROSE_WORD_LIMIT,
} from './obsidian-ai-provenance-core.mjs';

const MODEL = 'claude-fable-5-1';
const DATE = '2026-10-05';
const FOX_LINES = ['The quick brown fox.', '', '- jumps', '- over'];
const FOX_HASH = '4093afd5b43a';
const LONG_PROSE = Array.from({ length: UNMARKED_PROSE_WORD_LIMIT + 5 }, (_, index) => `word${index}`).join(' ');

describe('hashing', () => {
	it('matches the documented test vectors', () => {
		assert.equal(hashText('hello\nworld'), '26c60a61d01d');
		assert.equal(hashText(normalizeLines(FOX_LINES)), FOX_HASH);
	});
	it('ignores trailing whitespace, blank edges and the blockquote prefix', () => {
		assert.equal(normalizeLines(['', '> hello  ', '> world', '> ', ''], '> '), 'hello\nworld');
		assert.equal(normalizeLines(['hello\r', 'world']), 'hello\nworld');
	});
});

describe('parseMarkers', () => {
	it('finds a note marker after frontmatter', () => {
		const parsed = parseMarkers(['---', 'tags: [x]', '---', formatMarker('note', '', MODEL, DATE, FOX_HASH), 'body'].join('\n'));
		assert.equal(parsed.bodyStart, 3);
		assert.equal(parsed.note?.line, 3);
		assert.equal(parsed.note?.hash, FOX_HASH);
		assert.deepEqual(parsed.problems, []);
		assert.equal(authorshipOf(parsed), 'ai');
	});
	it('finds bare and full blocks, with blockquote prefixes', () => {
		const text = ['intro', '%% ai:begin %%', 'x', '%% ai:end %%', '> ' + formatMarker('begin', '', MODEL, DATE, FOX_HASH), '> y', '> %% ai:end %%'].join('\n');
		const parsed = parseMarkers(text);
		assert.equal(parsed.blocks.length, 2);
		assert.equal(parsed.blocks[0]?.begin.isBare, true);
		assert.equal(parsed.blocks[1]?.begin.prefix, '> ');
		assert.equal(parsed.blocks[1]?.endLine, 6);
		assert.deepEqual(parsed.problems, []);
		assert.equal(authorshipOf(parsed), 'mixed');
	});
	it('reports structural problems', () => {
		const text = ['%% ai:end %%', '%% ai:begin %%', '%% ai:begin %%', '%% ai:bogus %%', 'text'].join('\n');
		const problems = parseMarkers(text).problems;
		assert.ok(problems.some((problem) => problem.includes('ai:end without ai:begin')));
		assert.ok(problems.some((problem) => problem.includes('inside the ai:begin block')));
		assert.ok(problems.some((problem) => problem.includes('malformed')));
		assert.ok(problems.some((problem) => problem.includes('without ai:end')));
	});
	it('ignores markers inside fenced code and flags a misplaced note marker', () => {
		const fenced = ['doc', '```', '%% ai:begin %%', '```'].join('\n');
		assert.deepEqual(parseMarkers(fenced).problems, []);
		assert.equal(parseMarkers(fenced).blocks.length, 0);
		const misplaced = ['first', '%% ai:note %%', 'body'].join('\n');
		assert.ok(parseMarkers(misplaced).problems[0]?.includes('first body line'));
	});
});

describe('stampNote', () => {
	it('inserts a note marker after frontmatter on a whole-file write', () => {
		const result = stampNote(['---', 'a: 1', '---', ...FOX_LINES].join('\n'), { model: MODEL, date: DATE, affected: 'all' });
		assert.equal(result.changed, true);
		assert.equal(result.text.split('\n')[3], formatMarker('note', '', MODEL, DATE, FOX_HASH));
	});
	it('inserts a note marker at line 1 without frontmatter', () => {
		const result = stampNote(FOX_LINES.join('\n'), { model: MODEL, date: DATE, affected: 'all' });
		assert.equal(result.text.split('\n')[0], formatMarker('note', '', MODEL, DATE, FOX_HASH));
	});
	it('fills bare blocks and leaves untouched blocks alone', () => {
		const stale = formatMarker('begin', '', 'old-model', '2026-01-01', '000000000000');
		const text = ['h', stale, 'edited by human', '%% ai:end %%', '', '%% ai:begin %%', ...FOX_LINES, '%% ai:end %%'].join('\n');
		const result = stampNote(text, { model: MODEL, date: DATE, affected: [{ start: 5, end: 10 }] });
		const lines = result.text.split('\n');
		assert.equal(lines[1], stale);
		assert.equal(lines[5], formatMarker('begin', '', MODEL, DATE, FOX_HASH));
		assert.deepEqual(result.stamped, ['ai:begin at line 6']);
	});
	it('refreshes an affected block whose hash changed, keeping one with an equal hash', () => {
		const current = formatMarker('begin', '', 'old-model', '2026-01-01', FOX_HASH);
		const text = [current, ...FOX_LINES, '%% ai:end %%'].join('\n');
		assert.equal(stampNote(text, { model: MODEL, date: DATE, affected: [{ start: 1, end: 1 }] }).changed, false);
		const edited = [current, 'changed', '%% ai:end %%'].join('\n');
		const result = stampNote(edited, { model: MODEL, date: DATE, affected: [{ start: 1, end: 1 }] });
		assert.equal(result.text.split('\n')[0], formatMarker('begin', '', MODEL, DATE, hashText('changed')));
	});
	it('refreshes the note hash when an ai note is edited and refuses malformed notes', () => {
		const text = [formatMarker('note', '', MODEL, '2026-01-01', '000000000000'), 'body'].join('\n');
		const result = stampNote(text, { model: MODEL, date: DATE, affected: [{ start: 1, end: 1 }] });
		assert.equal(result.text.split('\n')[0], formatMarker('note', '', MODEL, DATE, hashText('body')));
		const malformed = stampNote('%% ai:begin %%\nx', { model: MODEL, date: DATE, affected: 'all' });
		assert.equal(malformed.changed, false);
		assert.equal(malformed.problems.length, 1);
	});
});

describe('gateEdit', () => {
	const humanNote = 'Human intro.\n\n## Section\n\nHuman text here.';
	it('allows wrapped insertions and small edits', () => {
		assert.deepEqual(gateEdit({ fileText: humanNote, oldString: 'Human text here.', newString: `Human text here.\n\n%% ai:begin %%\n${LONG_PROSE}\n%% ai:end %%` }), { ok: true });
		assert.deepEqual(gateEdit({ fileText: humanNote, oldString: 'Human intro.', newString: 'Human introduction.' }), { ok: true });
	});
	it('denies unmarked insertions above the word limit', () => {
		const result = gateEdit({ fileText: humanNote, oldString: 'Human text here.', newString: `Human text here.\n${LONG_PROSE}` });
		assert.equal(result.ok, false);
		assert.ok(!result.ok && result.reason.includes('%% ai:begin %%'));
	});
	it('allows edits inside an existing block and anywhere in an ai note', () => {
		const mixed = ['h', formatMarker('begin', '', MODEL, DATE, FOX_HASH), ...FOX_LINES, '%% ai:end %%'].join('\n');
		assert.deepEqual(gateEdit({ fileText: mixed, oldString: '- over', newString: `- over\n${LONG_PROSE}` }), { ok: true });
		const aiNote = [formatMarker('note', '', MODEL, DATE, FOX_HASH), ...FOX_LINES].join('\n');
		assert.deepEqual(gateEdit({ fileText: aiNote, oldString: '- over', newString: `- over\n${LONG_PROSE}` }), { ok: true });
	});
	it('denies removing or rewriting a marker', () => {
		const marker = formatMarker('begin', '', MODEL, DATE, FOX_HASH);
		const mixed = ['h', marker, 'x', '%% ai:end %%'].join('\n');
		assert.equal(gateEdit({ fileText: mixed, oldString: `${marker}\nx`, newString: 'x' }).ok, false);
		assert.equal(gateEdit({ fileText: mixed, oldString: marker, newString: formatMarker('begin', '', MODEL, DATE, '000000000000') }).ok, false);
		assert.equal(gateEdit({ fileText: mixed, oldString: `${marker}\nx`, newString: `${marker}\ny` }).ok, true);
	});
});

describe('gateWrite', () => {
	it('allows new files and ai notes, denies human and mixed notes', () => {
		assert.deepEqual(gateWrite({ existingText: undefined }), { ok: true });
		assert.deepEqual(gateWrite({ existingText: `${formatMarker('note', '', MODEL, DATE, FOX_HASH)}\nx` }), { ok: true });
		assert.equal(gateWrite({ existingText: 'human' }).ok, false);
		assert.equal(gateWrite({ existingText: '%% ai:begin %%\nx\n%% ai:end %%' }).ok, false);
	});
});

describe('vault audit', () => {
	it('lists only content notes and reports unit status', () => {
		const vault = mkdtempSync(path.join(tmpdir(), 'ai-provenance-'));
		try {
			mkdirSync(path.join(vault, '.obsidian'));
			mkdirSync(path.join(vault, '_templates'));
			mkdirSync(path.join(vault, 'Projects'));
			writeFileSync(path.join(vault, '_templates', 'ai-block.md'), '%% ai:begin %%');
			writeFileSync(path.join(vault, 'CLAUDE.md'), '%% ai:begin %%');
			writeFileSync(path.join(vault, 'human.md'), 'plain');
			writeFileSync(path.join(vault, 'Projects', 'mixed.md'), ['h', formatMarker('begin', '', MODEL, DATE, FOX_HASH), 'revised', '%% ai:end %%', '%% ai:begin %%', 'x', '%% ai:end %%'].join('\n'));
			writeFileSync(path.join(vault, 'ai.md'), [formatMarker('note', '', MODEL, DATE, FOX_HASH), ...FOX_LINES].join('\n'));
			assert.deepEqual(listVaultNotes(vault).map((file) => path.relative(vault, file)), ['Projects/mixed.md', 'ai.md', 'human.md']);
			assert.equal(resolveVaultNote(path.join(vault, '_templates', 'ai-block.md')), undefined);
			assert.equal(resolveVaultNote(path.join(vault, 'CLAUDE.md')), undefined);
			assert.equal(resolveVaultNote(path.join(vault, 'Projects', 'mixed.md')), vault);
			assert.equal(resolveVaultNote(path.join(tmpdir(), 'outside.md')), undefined);
			const reports = checkVault(vault);
			assert.deepEqual(reports.map((report) => [report.relativePath, report.authorship, report.units.map((unit) => unit.status)]), [
				['Projects/mixed.md', 'mixed', ['revised', 'unhashed']],
				['ai.md', 'ai', ['intact']],
			]);
			const index = renderIndex(reports, { generatedAt: DATE, refreshCommand: 'cmd' });
			assert.ok(index.includes('| [[Projects/mixed]] | mixed | claude-fable-5-1 | 2026-10-05 | 2 | revised 1/2 | 0/2 | unverified 2 |'));
			assert.ok(index.includes('| [[ai]] | ai | claude-fable-5-1 | 2026-10-05 | 1 | intact | 0/1 | unverified 1 |'));
			assert.ok(index.startsWith('%% generated by'));
		} finally {
			rmSync(vault, { recursive: true, force: true });
		}
	});
});
