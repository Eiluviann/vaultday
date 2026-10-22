import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { describe, it } from 'node:test';
import { resolveVaultNote } from './obsidian-ai-provenance-core.mjs';
import {
	authorKindOf,
	classifyUnmarkedChange,
	formatMarker,
	gateEdit,
	gateWrite,
	findWorkspaceRoot,
	isStampExempt,
	hashText,
	isUncheckedNoteSystem,
	parseMarkerLine,
	parseMarkers,
	renderIndex,
	renderReviewQueue,
	reportNoteText,
	reviewQueue,
	stampNote,
	verificationLines,
	verificationOf,
	withVerification,
} from './obsidian-ai-provenance-core.mjs';

const MODEL = 'claude-fable-5-1';
const LOCAL_MODEL = 'qwen3-coder:30b';
const TOOL = 'tool:vaultday';
const DATE = '2026-10-15';
const LATER = '2026-10-22';
const FOX_LINES = ['The quick brown fox.', '', '- jumps', '- over'];
const FOX_HASH = '4093afd5b43a';
const OTHER_HASH = '26c60a61d01d';
const VERIFIED_FOX = formatMarker('begin', '', MODEL, DATE, FOX_HASH, { date: LATER, hash: FOX_HASH });
const OPTIONS = { generatedAt: LATER, refreshCommand: 'cmd' };

/** @param {number} count */
function prose(count) {
	return Array.from({ length: count }, (_, index) => `word${index}`).join(' ');
}

describe('grammar', () => {
	it('parses verified note and block markers, also inside a blockquote', () => {
		const note = parseMarkerLine(formatMarker('note', '', LOCAL_MODEL, DATE, FOX_HASH, { date: LATER, hash: OTHER_HASH }), 0);
		assert.equal(note?.model, LOCAL_MODEL);
		assert.equal(note?.verifiedDate, LATER);
		assert.equal(note?.verifiedHash, OTHER_HASH);
		assert.equal(note?.isBare, false);
		const quoted = parseMarkers(['intro', `> ${VERIFIED_FOX}`, '> The quick brown fox.', '> ', '> - jumps', '> - over', '> %% ai:end %%'].join('\n'));
		assert.deepEqual(quoted.problems, []);
		assert.equal(quoted.blocks[0]?.begin.prefix, '> ');
		assert.equal(quoted.blocks[0]?.begin.verifiedHash, FOX_HASH);
	});
	it('keeps the three-field form valid and unverified', () => {
		const marker = parseMarkerLine(`%% ai:begin ${MODEL} · ${DATE} · ${FOX_HASH} %%`, 4);
		assert.equal(marker?.hash, FOX_HASH);
		assert.equal(marker?.verifiedHash, undefined);
		assert.equal(marker?.line, 4);
	});
	it('parses author-only bare markers and tool authors', () => {
		const bare = parseMarkerLine(`%% ai:begin ${LOCAL_MODEL} %%`, 0);
		assert.equal(bare?.isBare, true);
		assert.equal(bare?.model, LOCAL_MODEL);
		assert.equal(parseMarkerLine(`%% ai:note ${TOOL} · ${DATE} · ${FOX_HASH} %%`, 0)?.model, TOOL);
		assert.equal(authorKindOf(TOOL), 'tool');
		assert.equal(authorKindOf(LOCAL_MODEL), 'model');
	});
	it('rejects authors with whitespace or the separator', () => {
		assert.throws(() => formatMarker('note', '', 'two words', DATE, FOX_HASH));
		assert.throws(() => formatMarker('note', '', 'a·b', DATE, FOX_HASH));
		assert.throws(() => stampNote('x', { model: 'two words', date: DATE, affected: 'all' }));
		assert.equal(parseMarkerLine(`%% ai:begin a·b · ${DATE} · ${FOX_HASH} %%`, 0), undefined);
	});
	it('tolerates CRLF line endings', () => {
		const parsed = parseMarkers(`${formatMarker('note', '', MODEL, DATE, FOX_HASH)}\r\nbody\r\n`);
		assert.equal(parsed.note?.hash, FOX_HASH);
		assert.deepEqual(parsed.problems, []);
	});
});

describe('verification semantics', () => {
	it('derives unverified, verified and stale', () => {
		const bare = parseMarkerLine('%% ai:begin %%', 0);
		const stamped = parseMarkerLine(formatMarker('begin', '', MODEL, DATE, FOX_HASH), 0);
		const verified = parseMarkerLine(VERIFIED_FOX, 0);
		const verifiedAfterRevision = parseMarkerLine(formatMarker('begin', '', MODEL, DATE, FOX_HASH, { date: LATER, hash: OTHER_HASH }), 0);
		assert.ok(bare && stamped && verified && verifiedAfterRevision);
		assert.equal(verificationOf(bare, FOX_HASH), 'unverified');
		assert.equal(verificationOf(stamped, FOX_HASH), 'unverified');
		assert.equal(verificationOf(stamped, OTHER_HASH), 'unverified');
		assert.equal(verificationOf(verified, FOX_HASH), 'verified');
		assert.equal(verificationOf(verified, OTHER_HASH), 'stale');
		assert.equal(verificationOf(verifiedAfterRevision, OTHER_HASH), 'verified');
	});
	it('adds and removes the token without touching the stamp', () => {
		const line = `> ${formatMarker('begin', '', LOCAL_MODEL, DATE, FOX_HASH)}`;
		const verified = withVerification(line, { date: LATER, hash: FOX_HASH });
		assert.equal(verified, `> ${formatMarker('begin', '', LOCAL_MODEL, DATE, FOX_HASH, { date: LATER, hash: FOX_HASH })}`);
		assert.equal(withVerification(verified, undefined), line);
		assert.throws(() => withVerification('%% ai:begin %%', { date: LATER, hash: FOX_HASH }));
		assert.deepEqual(verificationLines(`a\n${verified}\n${line}`), [verified]);
	});
});

describe('stamping with verification and authors', () => {
	it('keeps the token when an agent refreshes a verified unit, so it reads as stale', () => {
		const text = [VERIFIED_FOX, 'changed', '%% ai:end %%'].join('\n');
		const result = stampNote(text, { model: LOCAL_MODEL, date: LATER, affected: [{ start: 1, end: 1 }] });
		const marker = parseMarkerLine(result.text.split('\n')[0] ?? '', 0);
		assert.equal(marker?.model, LOCAL_MODEL);
		assert.equal(marker?.hash, hashText('changed'));
		assert.equal(marker?.verifiedHash, FOX_HASH);
		assert.equal(verificationOf(marker ?? { kind: 'begin', line: 0, prefix: '', isBare: true }, hashText('changed')), 'stale');
	});
	it('keeps the author of an author-only bare marker and never adds a token', () => {
		const text = ['h', `%% ai:begin ${LOCAL_MODEL} %%`, ...FOX_LINES, '%% ai:end %%'].join('\n');
		const result = stampNote(text, { model: MODEL, date: DATE, affected: [{ start: 2, end: 5 }] });
		assert.equal(result.text.split('\n')[1], formatMarker('begin', '', LOCAL_MODEL, DATE, FOX_HASH));
		const inserted = stampNote(FOX_LINES.join('\n'), { model: TOOL, date: DATE, affected: 'all' });
		assert.equal(inserted.text.split('\n')[0], formatMarker('note', '', TOOL, DATE, FOX_HASH));
	});
});

describe('human-only verification gate', () => {
	const marker = formatMarker('begin', '', MODEL, DATE, FOX_HASH);
	const mixed = ['h', marker, 'x', '%% ai:end %%'].join('\n');
	const verifiedMixed = ['h', VERIFIED_FOX, ...FOX_LINES, '%% ai:end %%'].join('\n');
	it('denies adding or rewriting a verified token', () => {
		assert.equal(gateEdit({ fileText: mixed, oldString: marker, newString: VERIFIED_FOX }).ok, false);
		const added = gateEdit({ fileText: 'h', oldString: 'h', newString: `h\n${VERIFIED_FOX}\nx\n%% ai:end %%` });
		assert.equal(added.ok, false);
		assert.ok(!added.ok && added.reason.includes('human-only'));
		assert.equal(gateEdit({ fileText: verifiedMixed, oldString: VERIFIED_FOX, newString: withVerification(VERIFIED_FOX, undefined) }).ok, false);
	});
	it('allows editing inside a verified block', () => {
		assert.deepEqual(gateEdit({ fileText: verifiedMixed, oldString: '- over', newString: `- over\n${prose(40)}` }), { ok: true });
	});
	it('gateWrite keeps verified lines and allows them verbatim', () => {
		const aiNote = [formatMarker('note', '', MODEL, DATE, FOX_HASH, { date: LATER, hash: FOX_HASH }), ...FOX_LINES].join('\n');
		assert.equal(gateWrite({ existingText: aiNote, newText: 'rewritten' }).ok, false);
		assert.deepEqual(gateWrite({ existingText: aiNote, newText: `${aiNote}\nmore` }), { ok: true });
		assert.deepEqual(gateWrite({ existingText: aiNote }), { ok: true });
	});
});

describe('strict gate for v1 vaults', () => {
	const strict = { isStrict: true };
	const note = ['---', 'status: draft', 'tags: []', '---', '# Title', '', 'Human paragraph here.', '', '## Next Steps', '- [ ] Call Janusz [created:: 2026-10-01] [assigned_to:: me]', '- [ ] Ship it #soon'].join('\n');
	it('denies prose of any length', () => {
		assert.equal(gateEdit({ fileText: note, oldString: 'Human paragraph here.', newString: `Human paragraph here.\n${prose(26)}` }, strict).ok, false);
		const short = gateEdit({ fileText: note, oldString: 'Human paragraph here.', newString: `Human paragraph here.\n${prose(10)}` }, strict);
		assert.equal(short.ok, false);
		assert.ok(!short.ok && short.reason.includes('new line'));
		assert.equal(gateEdit({ fileText: note, oldString: '- [ ] Ship it #soon', newString: '- [ ] Ship it #soon\n- Also do this\n- And that' }, strict).ok, false);
	});
	it('allows field-level changes', () => {
		assert.deepEqual(gateEdit({ fileText: note, oldString: '- [ ] Call Janusz', newString: '- [x] Call Janusz' }, strict), { ok: true });
		assert.deepEqual(gateEdit({ fileText: note, oldString: '[assigned_to:: me]', newString: '[assigned_to:: me] [due:: 2026-10-30]' }, strict), { ok: true });
		assert.deepEqual(gateEdit({ fileText: note, oldString: 'status: draft', newString: 'status: ready' }, strict), { ok: true });
		assert.deepEqual(gateEdit({ fileText: note, oldString: '- [ ] Ship it #soon', newString: '- [ ] Ship it #soon #blocked' }, strict), { ok: true });
		assert.deepEqual(gateEdit({ fileText: note, oldString: '## Next Steps', newString: '## Next Steps\n- 2026-10-22 Agreed the mapping with Janusz [kind:: decision]' }, strict), { ok: true });
		assert.deepEqual(gateEdit({ fileText: note, oldString: '## Next Steps', newString: '## Next Steps\n- 16:40 session · Drafted testing steps [kind:: session] [agent:: claude]' }, strict), { ok: true });
		assert.deepEqual(gateEdit({ fileText: note, oldString: 'Human paragraph here.', newString: 'Human paragraph is here.' }, strict), { ok: true });
	});
	it('lets an attributed log line through at any length, but not an unattributed one', () => {
		const longLine = `- 10:45 decision · ${prose(30)} [kind:: decision] [story:: STRY0682156] [agent:: claude-fable-5-1]`;
		assert.deepEqual(gateEdit({ fileText: note, oldString: '## Next Steps', newString: `## Next Steps\n${longLine}` }, strict), { ok: true });
		const unattributed = `- 10:45 decision · ${prose(30)} [kind:: decision] [story:: STRY0682156]`;
		assert.equal(gateEdit({ fileText: note, oldString: '## Next Steps', newString: `## Next Steps\n${unattributed}` }, strict).ok, false);
	});
	it('keeps the word limit outside strict mode and classifies deletions as none', () => {
		assert.deepEqual(gateEdit({ fileText: note, oldString: 'Human paragraph here.', newString: `Human paragraph here.\n${prose(10)}` }), { ok: true });
		const parsed = parseMarkers(note);
		assert.equal(classifyUnmarkedChange(parsed, 6, 'Human paragraph here.', '').kind, 'none');
		assert.equal(classifyUnmarkedChange(parsed, 6, 'Human paragraph here.', `Human paragraph here.\n%% ai:begin %%\n${prose(40)}\n%% ai:end %%`).kind, 'none');
	});
});

describe('reports and generated notes', () => {
	const stale = formatMarker('begin', '', LOCAL_MODEL, '2026-10-01', OTHER_HASH, { date: '2026-10-02', hash: OTHER_HASH });
	const text = ['---', 'type: story', '---', '# Story', '', '## Background', VERIFIED_FOX, ...FOX_LINES, '%% ai:end %%', '', '## Implementation', stale, 'edited since', '%% ai:end %%', '', `%% ai:begin ${TOOL} · 2026-10-20 · ${hashText('fact line')} %%`, 'fact line', '%% ai:end %%'].join('\n');
	const report = reportNoteText('Projects/P/Story.md', text);
	it('fills verification, author kind, heading and word count per unit', () => {
		assert.deepEqual(
			report.units.map((unit) => [unit.verification, unit.authorKind, unit.heading, unit.words, unit.status]),
			[
				['verified', 'model', 'Background', 6, 'intact'],
				['stale', 'model', 'Implementation', 2, 'revised'],
				['unverified', 'tool', 'Implementation', 2, 'intact'],
			],
		);
		assert.equal(report.units[1]?.verifiedDate, '2026-10-02');
	});
	it('queues unverified and stale units oldest first and skips agent material', () => {
		const agentReport = reportNoteText('Projects/P/_agent/research/raw.md', [`%% ai:note ${MODEL} · 2026-09-01 · ${hashText('raw')} %%`, 'raw'].join('\n'));
		const queue = reviewQueue([report, agentReport]);
		assert.deepEqual(queue.map((entry) => [entry.unit.date, entry.unit.verification]), [
			['2026-10-01', 'stale'],
			['2026-10-20', 'unverified'],
		]);
	});
	it('renders the index columns and the review queue list', () => {
		const index = renderIndex([report], OPTIONS);
		assert.ok(index.includes('| Note | Authorship | Last author | Last date | Units | Integrity | Verified | Verification |'));
		assert.ok(index.includes(`| [[Projects/P/Story]] | mixed | ${TOOL} | 2026-10-20 | 3 | revised 1/3 | 1/3 | stale 1 |`));
		const queueNote = renderReviewQueue([report], OPTIONS);
		assert.ok(queueNote.startsWith('---\ngenerated: 2026-10-22\nai_units: 3\nai_verified: 1\nai_unverified: 1\nai_stale: 1\n---\n'));
		assert.ok(queueNote.includes(`- [[Projects/P/Story#Implementation|Story · Implementation]] [state:: stale] [model:: ${LOCAL_MODEL}] [written:: 2026-10-01] [verified:: 2026-10-02] [words:: 2] [unit:: ${hashText('edited since')}]`));
		assert.ok(queueNote.includes(`[state:: unverified] [model:: ${TOOL}]`));
		assert.ok(!queueNote.includes('Background'));
		assert.ok(!queueNote.includes('[note_system::'));
		assert.ok(renderReviewQueue([], OPTIONS).includes('_nothing to review_'));
	});
	it('flags units of unchecked migrated notes in the queue', () => {
		const unchecked = reportNoteText('Old/Note.md', ['---', 'note_system: 0.1', '---', `%% ai:note ${MODEL} · 2026-09-01 · ${hashText('old claim')} %%`, 'old claim'].join('\n'));
		const legacyKey = reportNoteText('Old/Older.md', ['---', 'note-system-ver: 0.1', '---', 'human'].join('\n'));
		assert.equal(unchecked.noteSystem, '0.1');
		assert.equal(legacyKey.noteSystem, '0.1');
		assert.equal(report.noteSystem, undefined);
		assert.equal(isUncheckedNoteSystem('0.1'), true);
		assert.equal(isUncheckedNoteSystem('1.0'), false);
		assert.equal(isUncheckedNoteSystem(undefined), false);
		assert.ok(renderReviewQueue([unchecked], OPTIONS).includes(`[unit:: ${hashText('old claim')}] [note_system:: 0.1]`));
	});
});

describe('workspace detection', () => {
	it('finds the nearest Workspace.md above the note and skips the manifest itself', () => {
		const vault = mkdtempSync(path.join(tmpdir(), 'ai-provenance-ws-'));
		try {
			mkdirSync(path.join(vault, '.obsidian'));
			mkdirSync(path.join(vault, 'Projects', 'Coaching', 'Deep'), { recursive: true });
			writeFileSync(path.join(vault, 'Projects', 'Coaching', 'Workspace.md'), '---\ntype: workspace\n---\n');
			assert.equal(findWorkspaceRoot(path.join(vault, 'Projects', 'Coaching', 'Deep', 'note.md'), vault), path.join(vault, 'Projects', 'Coaching'));
			assert.equal(findWorkspaceRoot(path.join(vault, 'Projects', 'other.md'), vault), undefined);
			writeFileSync(path.join(vault, 'Workspace.md'), '---\ntype: workspace\n---\n');
			assert.equal(findWorkspaceRoot(path.join(vault, 'Projects', 'other.md'), vault), vault);
			assert.equal(resolveVaultNote(path.join(vault, 'Workspace.md')), undefined);
			assert.equal(resolveVaultNote(path.join(vault, 'Projects', 'Coaching', 'Workspace.md')), undefined);
			assert.equal(isStampExempt(path.join(vault, 'Log', '2026-10-22.md'), vault), true);
			assert.equal(isStampExempt(path.join(vault, 'Projects', 'Coaching', 'Log', '2026-10-22.md'), vault), true);
			assert.equal(isStampExempt(path.join(vault, 'Projects', 'Log.md'), vault), false);
		} finally {
			rmSync(vault, { recursive: true, force: true });
		}
	});
});
