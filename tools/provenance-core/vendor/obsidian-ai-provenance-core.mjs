/**
 * Core logic for the Obsidian AI-provenance convention: parsing `%% ai:… %%`
 * markers, fingerprinting their content, stamping bare markers, gating agent
 * edits, auditing a vault and rendering the index and review-queue notes.
 *
 * Markers are Obsidian comments, so they are invisible in Reading view and
 * visible in Live Preview / Source mode. Grammar:
 *   %% ai:note  <author> · <YYYY-MM-DD> · <12 hex> %%   first body line, hashes the rest of the note
 *   %% ai:begin <author> · <YYYY-MM-DD> · <12 hex> %%   … %% ai:end %%   hashes the lines in between
 *   … · <12 hex> · verified <YYYY-MM-DD> · <12 hex> %%   human verification: date and the hash the human checked
 * Bare markers (`%% ai:begin %%`, `%% ai:note %%`) and author-only bare markers
 * (`%% ai:begin qwen3-coder:30b %%`) are filled in by stampNote, keeping the author.
 * Authors are model ids (`claude-fable-5-1`, `qwen3-coder:30b`) or tools (`tool:vaultday`).
 * The verification token is written only by humans (Obsidian plugin command); an agent
 * edit inside a verified unit refreshes the stamp and leaves the token, so the unit reads
 * as `stale` until a human verifies it again.
 *
 * Pure functions only; the hook/CLI entry point lives in ../obsidian-ai-provenance.mjs.
 */
import { createHash } from 'node:crypto';
import { existsSync, readdirSync, readFileSync } from 'node:fs';
import path from 'node:path';

// ---------------------------------------------------------------- types

/**
 * @typedef {object} Marker
 * @property {'note' | 'begin'} kind
 * @property {number} line 0-based line index of the marker
 * @property {string} prefix blockquote prefix (`> ` repeated) or ''
 * @property {string} [model] author: model id or `tool:<name>`
 * @property {string} [date]
 * @property {string} [hash]
 * @property {string} [verifiedDate]
 * @property {string} [verifiedHash]
 * @property {boolean} isBare true when date and hash are missing (an author alone may be present)
 */
/**
 * @typedef {object} Block
 * @property {Marker} begin
 * @property {number} endLine 0-based line index of the `ai:end` marker
 */
/**
 * @typedef {object} ParsedNote
 * @property {string[]} lines
 * @property {number} bodyStart index of the first line after the frontmatter (0 without frontmatter)
 * @property {Marker | undefined} note
 * @property {Block[]} blocks
 * @property {string[]} problems human-readable structural problems, empty when well-formed
 */
/** @typedef {{ start: number, end: number }} LineRange 0-based, inclusive */
/** @typedef {'all' | LineRange[]} AffectedLines */
/** @typedef {'intact' | 'revised' | 'unhashed'} UnitStatus */
/** @typedef {'unverified' | 'verified' | 'stale'} VerificationStatus */
/** @typedef {'model' | 'tool'} AuthorKind */
/** @typedef {{ date: string, hash: string }} Verification */
/**
 * @typedef {object} UnitReport
 * @property {'note' | 'block'} kind
 * @property {number} line 1-based line of the marker
 * @property {string} model author as written in the marker
 * @property {AuthorKind} authorKind
 * @property {string} date
 * @property {UnitStatus} status integrity of the stamp against the current text
 * @property {string} currentHash
 * @property {VerificationStatus} verification
 * @property {string} [verifiedDate]
 * @property {number} words word count of the unit's content
 * @property {string} [heading] nearest heading above the unit, blocks only
 */
/**
 * @typedef {object} NoteReport
 * @property {string} relativePath vault-relative path, forward slashes
 * @property {'ai' | 'mixed' | 'human'} authorship
 * @property {string | undefined} noteSystem frontmatter `note_system` (or legacy `note-system-ver`) value; below 1.0 the note is migrated and unchecked, lower fact weight
 * @property {UnitReport[]} units
 * @property {string[]} problems
 */
/** @typedef {{ ok: true } | { ok: false, reason: string }} GateResult */
/** @typedef {{ isStrict?: boolean }} GateEditOptions isStrict: v1-vault rule, prose of any length must sit inside markers */
/** @typedef {{ kind: 'none' | 'field' | 'prose', category: string, words: number }} ChangeClass */
/**
 * @typedef {object} StampResult
 * @property {string} text
 * @property {boolean} changed
 * @property {string[]} stamped descriptions of the markers that were written
 * @property {string[]} problems structural problems that prevented stamping
 */
/**
 * @typedef {object} QueueEntry
 * @property {string} relativePath
 * @property {UnitReport} unit
 * @property {string | undefined} noteSystem
 */

// ---------------------------------------------------------------- constants

export const FIELD_CHANGE_WORD_LIMIT = 25;
/** @deprecated alias kept for callers of the pre-verification API */
export const UNMARKED_PROSE_WORD_LIMIT = FIELD_CHANGE_WORD_LIMIT;
export const TOUCH_UP_WORD_LIMIT = 3;
export const TOUCH_UP_LINE_LIMIT = 2;
export const HASH_LENGTH = 12;
export const INDEX_NOTE_NAMES = ['AI provenance.md', 'ai-provenance.md'];
export const REVIEW_QUEUE_NOTE_NAME = 'Review queue.md';
export const WORKSPACE_NOTE_NAME = 'Workspace.md';
export const SKIPPED_DIRECTORY_NAMES = new Set([
	'.obsidian',
	'.trash',
	'.git',
	'.claude',
	'_templates',
	'templates',
	'attachments',
	'_resources',
	'node_modules',
	'dist',
]);
export const SKIPPED_BASENAMES = new Set(['CLAUDE.md', 'AGENTS.md', WORKSPACE_NOTE_NAME, ...INDEX_NOTE_NAMES, REVIEW_QUEUE_NOTE_NAME]);
/** Folders whose units never enter the review queue (agent working material, day logs). */
export const QUEUE_EXCLUDED_SEGMENTS = new Set(['_agent', 'Log']);
/**
 * Folders whose notes are gated but never stamped: day logs attribute each line
 * with an `[agent:: …]` field instead of markers, so a whole-file `ai:note` or a
 * refreshed block stamp would misstate who wrote what.
 */
export const STAMP_EXEMPT_SEGMENTS = new Set(['Log']);
export const UNKNOWN_FIELD = '?';
export const TOOL_AUTHOR_PREFIX = 'tool:';

const AUTHOR_PATTERN = /^[^\s·]+$/;
const DATE_PATTERN = /^\d{4}-\d{2}-\d{2}$/;
const MARKER_PATTERN =
	/^(?<prefix>(?:> )*)%% ai:(?<kind>note|begin)(?: (?<model>[^\s·]+)(?: · (?<date>\d{4}-\d{2}-\d{2}) · (?<hash>[0-9a-f]{12})(?: · verified (?<verifiedDate>\d{4}-\d{2}-\d{2}) · (?<verifiedHash>[0-9a-f]{12}))?)?)? %%$/;
const END_PATTERN = /^(?<prefix>(?:> )*)%% ai:end %%$/;
const ANY_MARKER_PATTERN = /^(?:> )*%% ai:/;
const FENCE_PATTERN = /^(?:> )*\s*(?:```|~~~)/;
const HEADING_PATTERN = /^(?:> )*(#{1,6})\s+(.*?)\s*#*\s*$/;
const FRONTMATTER_DELIMITERS = new Set(['---', '...']);
const NOTE_SYSTEM_FRONTMATTER_PATTERN = /^(?:note_system|note-system-ver):\s*(\S+)\s*$/;
const CHECKBOX_PATTERN = /^(\s*[-*+]\s+\[)[^\]]\]/;
const INLINE_FIELD_PATTERN = /\[[^[\]:]+::[^\]]*\]|\([^():]+::[^)]*\)/g;
const WHOLE_LINE_FIELD_PATTERN = /^\s*(?:[-*+]\s+)?([\w .-]+?)::\s/;
const TABLE_ROW_PATTERN = /^\s*\|.*\|\s*$/;
const HEADING_LINE_PATTERN = /^(?:> )*#{1,6}\s/;
const DATED_ITEM_PATTERN = /^\s*(?:[-*+]|\d+[.)])\s+(?:\[.\]\s+)?(?:\d{4}-\d{2}-\d{2}|\[\[\d{4}-\d{2}-\d{2}\]\]|\d{1,2}:\d{2})\b/;
const AGENT_FIELD_PATTERN = /\[agent::\s*\S+\]/;
const TAG_TOKEN_PATTERN = /^#[\p{L}\p{N}_/-]+[.,;:]?$/u;
const LINK_TOKEN_PATTERN = /^!?\[\[[^\]]*\]\][.,;:]?$/;

// ---------------------------------------------------------------- parsing

/**
 * Parses a note's text into its provenance markers.
 * Markers inside fenced code blocks are ignored so documentation can quote them.
 * @param {string} text
 * @returns {ParsedNote}
 */
export function parseMarkers(text) {
	const lines = text.split('\n');
	const bodyStart = findBodyStart(lines);
	/** @type {ParsedNote} */
	const parsed = { lines, bodyStart, note: undefined, blocks: [], problems: [] };
	const firstBodyLine = findFirstNonBlankLine(lines, bodyStart);
	/** @type {Marker | undefined} */
	let openBlock;
	let isInsideFence = false;
	for (let index = bodyStart; index < lines.length; index++) {
		const line = stripCarriageReturn(lines[index] ?? '');
		if (FENCE_PATTERN.test(line)) {
			isInsideFence = !isInsideFence;
			continue;
		}
		if (isInsideFence) continue;
		const marker = parseMarkerLine(line, index);
		if (marker !== undefined) {
			if (openBlock !== undefined) {
				parsed.problems.push(`line ${index + 1}: ai:${marker.kind} inside the ai:begin block opened at line ${openBlock.line + 1}`);
			} else if (marker.kind === 'begin') {
				openBlock = marker;
			} else if (parsed.note !== undefined) {
				parsed.problems.push(`line ${index + 1}: second ai:note marker (first at line ${parsed.note.line + 1})`);
			} else {
				if (index !== firstBodyLine) parsed.problems.push(`line ${index + 1}: ai:note marker must be the first body line`);
				parsed.note = marker;
			}
			continue;
		}
		const endMatch = END_PATTERN.exec(line);
		if (endMatch !== null) {
			if (openBlock === undefined) {
				parsed.problems.push(`line ${index + 1}: ai:end without ai:begin`);
			} else {
				if ((endMatch.groups?.prefix ?? '') !== openBlock.prefix) {
					parsed.problems.push(`line ${index + 1}: ai:end blockquote prefix differs from ai:begin at line ${openBlock.line + 1}`);
				}
				parsed.blocks.push({ begin: openBlock, endLine: index });
				openBlock = undefined;
			}
			continue;
		}
		if (ANY_MARKER_PATTERN.test(line)) parsed.problems.push(`line ${index + 1}: malformed ai marker`);
	}
	if (openBlock !== undefined) parsed.problems.push(`line ${openBlock.line + 1}: ai:begin without ai:end`);
	return parsed;
}

/**
 * Parses one line as a marker; undefined when the line is not a marker.
 * @param {string} rawLine
 * @param {number} index 0-based line index to record
 * @returns {Marker | undefined}
 */
export function parseMarkerLine(rawLine, index) {
	const match = MARKER_PATTERN.exec(stripCarriageReturn(rawLine));
	if (match === null || match.groups === undefined) return undefined;
	const { prefix, kind, model, date, hash, verifiedDate, verifiedHash } = match.groups;
	if (kind !== 'note' && kind !== 'begin') return undefined;
	return { kind, line: index, prefix: prefix ?? '', model, date, hash, verifiedDate, verifiedHash, isBare: hash === undefined };
}

/**
 * Derives the note's authorship from its markers: `ai:note` → ai, blocks only → mixed, none → human.
 * @param {ParsedNote} parsed
 * @returns {'ai' | 'mixed' | 'human'}
 */
export function authorshipOf(parsed) {
	if (parsed.note !== undefined) return 'ai';
	if (parsed.blocks.length > 0) return 'mixed';
	return 'human';
}

/**
 * `tool:<name>` authors are tools; every other author token is a model id.
 * @param {string} author
 * @returns {AuthorKind}
 */
export function authorKindOf(author) {
	return author.startsWith(TOOL_AUTHOR_PREFIX) ? 'tool' : 'model';
}

/**
 * Verification state of a unit against the hash of its current content.
 * @param {Marker} marker
 * @param {string} currentHash
 * @returns {VerificationStatus}
 */
export function verificationOf(marker, currentHash) {
	if (marker.verifiedHash === undefined) return 'unverified';
	return marker.verifiedHash === currentHash ? 'verified' : 'stale';
}

// ---------------------------------------------------------------- hashing

/**
 * sha256 of the normalized text, truncated to HASH_LENGTH hex characters.
 * @param {string} normalizedText
 */
export function hashText(normalizedText) {
	return createHash('sha256').update(normalizedText, 'utf8').digest('hex').slice(0, HASH_LENGTH);
}

/**
 * Normalizes unit lines for hashing: strip the blockquote prefix, right-trim
 * every line, drop leading and trailing blank lines, join with '\n'.
 * @param {string[]} lines
 * @param {string} [prefix]
 */
export function normalizeLines(lines, prefix = '') {
	const stripped = lines.map((line) => (prefix.length > 0 && line.startsWith(prefix) ? line.slice(prefix.length) : line).trimEnd());
	return trimBlankEdges(stripped).join('\n');
}

/**
 * Hash of a block's body (the lines strictly between its markers).
 * @param {string[]} lines
 * @param {Block} block
 */
export function blockBodyHash(lines, block) {
	return hashText(normalizeLines(lines.slice(block.begin.line + 1, block.endLine), block.begin.prefix));
}

/**
 * Hash of a note's body (every line after the `ai:note` marker).
 * @param {string[]} lines
 * @param {number} noteMarkerLine
 */
export function noteBodyHash(lines, noteMarkerLine) {
	return hashText(normalizeLines(lines.slice(noteMarkerLine + 1)));
}

// ---------------------------------------------------------------- stamping

/**
 * Fills bare markers and refreshes the fingerprints of affected units with the
 * given author and date. An author-only bare marker keeps its author. Units
 * outside the affected lines keep their existing marker untouched so later human
 * edits elsewhere stay detectable. A verification token is always carried over,
 * never added. With `affected: 'all'` (a whole-file Write) a missing `ai:note`
 * marker is inserted after the frontmatter.
 * @param {string} text
 * @param {{ model: string, date: string, affected: AffectedLines }} options
 * @returns {StampResult}
 */
export function stampNote(text, options) {
	const { model, date, affected } = options;
	if (!AUTHOR_PATTERN.test(model)) throw new Error('model must be a non-empty author token without whitespace or "·"');
	if (!DATE_PATTERN.test(date)) throw new Error('date must be YYYY-MM-DD');
	const parsed = parseMarkers(text);
	if (parsed.problems.length > 0) return { text, changed: false, stamped: [], problems: parsed.problems };

	const lines = [...parsed.lines];
	/** @type {string[]} */
	const stamped = [];
	for (const block of parsed.blocks) {
		if (!isUnitAffected(block.begin, block.endLine, affected)) continue;
		const hash = blockBodyHash(lines, block);
		if (!block.begin.isBare && block.begin.hash === hash) continue;
		lines[block.begin.line] = formatMarker('begin', block.begin.prefix, authorForStamp(block.begin, model), date, hash, verificationOfMarker(block.begin));
		stamped.push(`ai:begin at line ${block.begin.line + 1}`);
	}
	if (parsed.note !== undefined) {
		if (isUnitAffected(parsed.note, lines.length - 1, affected)) {
			const hash = noteBodyHash(lines, parsed.note.line);
			if (parsed.note.isBare || parsed.note.hash !== hash) {
				lines[parsed.note.line] = formatMarker('note', parsed.note.prefix, authorForStamp(parsed.note, model), date, hash, verificationOfMarker(parsed.note));
				stamped.push(`ai:note at line ${parsed.note.line + 1}`);
			}
		}
	} else if (affected === 'all') {
		const hash = hashText(normalizeLines(lines.slice(parsed.bodyStart)));
		lines.splice(parsed.bodyStart, 0, formatMarker('note', '', model, date, hash));
		stamped.push(`ai:note inserted at line ${parsed.bodyStart + 1}`);
	}
	const nextText = lines.join('\n');
	return { text: nextText, changed: nextText !== text, stamped, problems: [] };
}

/**
 * Formats a canonical marker line.
 * @param {'note' | 'begin'} kind
 * @param {string} prefix
 * @param {string} author model id or `tool:<name>`
 * @param {string} date
 * @param {string} hash
 * @param {Verification} [verification]
 * @throws when the author contains whitespace or the field separator
 */
export function formatMarker(kind, prefix, author, date, hash, verification) {
	if (!AUTHOR_PATTERN.test(author)) throw new Error(`invalid author "${author}": no whitespace or "·" allowed`);
	const stamp = `${prefix}%% ai:${kind} ${author} · ${date} · ${hash}`;
	return verification === undefined ? `${stamp} %%` : `${stamp} · verified ${verification.date} · ${verification.hash} %%`;
}

/**
 * Rewrites a stamped marker line with the given verification, or without one.
 * @param {string} markerLine
 * @param {Verification | undefined} verification
 * @throws when the line is not a stamped marker
 */
export function withVerification(markerLine, verification) {
	const marker = parseMarkerLine(markerLine, 0);
	if (marker === undefined || marker.model === undefined || marker.date === undefined || marker.hash === undefined) {
		throw new Error('withVerification needs a stamped marker line');
	}
	return formatMarker(marker.kind, marker.prefix, marker.model, marker.date, marker.hash, verification);
}

// ---------------------------------------------------------------- gating

/**
 * Decides whether an Edit on a vault note may proceed: markers must not be
 * removed or rewritten, verification tokens are human-only, and new prose must
 * sit inside ai markers unless the edit lands in an existing ai unit. Outside
 * strict mode, unmarked additions under FIELD_CHANGE_WORD_LIMIT words pass; in
 * strict mode (v1 vaults) only field-level changes pass unmarked.
 * @param {{ fileText: string, oldString: string, newString: string }} edit
 * @param {GateEditOptions} [options]
 * @returns {GateResult}
 */
export function gateEdit(edit, options = {}) {
	const { fileText, oldString, newString } = edit;
	const isStrict = options.isStrict ?? false;
	if (removesMarkerLines(oldString, newString)) {
		return {
			ok: false,
			reason:
				'This edit removes or rewrites a `%% ai:… %%` provenance marker. Markers are maintained by the provenance hook and must never be edited, moved or deleted by an agent. Leave the marker lines exactly as they are.',
		};
	}
	if (addsVerification(oldString, newString)) {
		return {
			ok: false,
			reason:
				'This edit writes a `verified` token into a provenance marker. Verification is human-only: the user marks a passage as verified from Obsidian. Write the text and leave verification to them.',
		};
	}
	const parsed = parseMarkers(fileText);
	if (parsed.note !== undefined) return { ok: true };
	const matchLine = locateLine(fileText, oldString);
	if (matchLine !== undefined && parsed.blocks.some((block) => block.begin.line <= matchLine && matchLine <= block.endLine)) {
		return { ok: true };
	}
	if (!isStrict) {
		const addedWords = wordCount(unmarkedAddedLines(oldString, newString));
		if (addedWords < FIELD_CHANGE_WORD_LIMIT) return { ok: true };
		return { ok: false, reason: proseDenialReason(`inserts ${addedWords} words of unmarked model-written text into a vault note (limit without markers: ${FIELD_CHANGE_WORD_LIMIT})`) };
	}
	const change = classifyUnmarkedChange(parsed, matchLine, oldString, newString);
	if (change.kind !== 'prose') return { ok: true };
	return { ok: false, reason: proseDenialReason(`adds unmarked text to a v1 vault note (${change.category}, ${change.words} words). Only field-level changes pass without markers: frontmatter values, checkbox state, inline [key:: value] fields, tag or link touch-ups, one dated log line, table row or heading, or a wording touch-up of at most ${TOUCH_UP_WORD_LIMIT} words`) };
}

/**
 * Classifies the unmarked lines an edit adds outside ai units.
 * @param {ParsedNote} parsed
 * @param {number | undefined} matchLine 0-based line where oldString starts
 * @param {string} oldString
 * @param {string} newString
 * @returns {ChangeClass}
 */
export function classifyUnmarkedChange(parsed, matchLine, oldString, newString) {
	const added = unmarkedAddedLines(oldString, newString).filter((line) => line.trim().length > 0);
	const words = wordCount(added);
	if (added.length === 0) return { kind: 'none', category: 'deletion or marker-wrapped text', words: 0 };
	if (matchLine !== undefined && matchLine + countNewlines(oldString) < parsed.bodyStart) {
		return words <= FIELD_CHANGE_WORD_LIMIT ? { kind: 'field', category: 'frontmatter', words } : { kind: 'prose', category: 'frontmatter over the word limit', words };
	}
	const firstAdded = added[0];
	if (added.length === 1 && firstAdded !== undefined && isStructuredEntry(firstAdded)) {
		// A dated or timed log line that names its author in `[agent:: …]` is attributed by that field; its length is free.
		if (DATED_ITEM_PATTERN.test(firstAdded) && AGENT_FIELD_PATTERN.test(firstAdded)) return { kind: 'field', category: 'attributed log line', words };
		return words <= FIELD_CHANGE_WORD_LIMIT ? { kind: 'field', category: 'structured entry', words } : { kind: 'prose', category: 'structured entry over the word limit', words };
	}
	const removedPool = removedLines(oldString, newString).filter((line) => line.trim().length > 0);
	let touchUps = 0;
	for (const line of added) {
		const partner = takeMostSimilar(removedPool, line);
		if (partner === undefined) return { kind: 'prose', category: 'new line', words: wordCount([line]) };
		if (isCheckboxToggle(partner, line)) continue;
		if (isInlineFieldChange(partner, line)) {
			if (wordCount([line]) > FIELD_CHANGE_WORD_LIMIT) return { kind: 'prose', category: 'inline-field line over the word limit', words: wordCount([line]) };
			continue;
		}
		if (isTagOrLinkTouchUp(partner, line)) continue;
		if (wordDistance(partner, line) <= TOUCH_UP_WORD_LIMIT) {
			touchUps++;
			if (touchUps > TOUCH_UP_LINE_LIMIT) return { kind: 'prose', category: `wording changes on more than ${TOUCH_UP_LINE_LIMIT} lines`, words };
			continue;
		}
		return { kind: 'prose', category: 'rewritten line', words: wordCount([line]) };
	}
	return { kind: 'field', category: 'field-level change', words };
}

/**
 * Decides whether a Write may replace an existing vault note: only new files
 * and notes already marked `ai:note` may be overwritten, and a verified marker
 * line must survive the write verbatim.
 * @param {{ existingText: string | undefined, newText?: string }} write
 * @returns {GateResult}
 */
export function gateWrite(write) {
	if (write.existingText === undefined) return { ok: true };
	if (parseMarkers(write.existingText).note === undefined) {
		return {
			ok: false,
			reason:
				'This vault note is human-written or mixed; overwriting it with Write would erase its authorship provenance. Use Edit instead and wrap inserted text in `%% ai:begin %%` … `%% ai:end %%` markers on their own lines.',
		};
	}
	if (write.newText !== undefined) {
		const newLines = new Set(write.newText.split('\n').map(stripCarriageReturn));
		const dropped = verificationLines(write.existingText).filter((line) => !newLines.has(line));
		if (dropped.length > 0) {
			return {
				ok: false,
				reason: `This write drops ${dropped.length} verified provenance marker line(s). Verification is human-only; keep every marker line that carries a \`verified\` token exactly as it is (the hook refreshes the stamp around it).`,
			};
		}
	}
	return { ok: true };
}

/**
 * Marker lines of the text that carry a verification token, as written.
 * @param {string} text
 */
export function verificationLines(text) {
	return text
		.split('\n')
		.map(stripCarriageReturn)
		.filter((line) => parseMarkerLine(line, 0)?.verifiedHash !== undefined);
}

// ---------------------------------------------------------------- auditing

/**
 * Audits every note in the vault and returns the notes that carry AI markers
 * or have structural problems.
 * @param {string} vaultRoot
 * @returns {NoteReport[]}
 */
export function checkVault(vaultRoot) {
	return listVaultNotes(vaultRoot)
		.map((filePath) => reportNote(vaultRoot, filePath))
		.filter((report) => report.authorship !== 'human' || report.problems.length > 0);
}

/**
 * Audits one note: authorship, per-unit fingerprint and verification status, problems.
 * @param {string} vaultRoot
 * @param {string} filePath
 * @returns {NoteReport}
 */
export function reportNote(vaultRoot, filePath) {
	return reportNoteText(path.relative(vaultRoot, filePath).split(path.sep).join('/'), readFileSync(filePath, 'utf8'));
}

/**
 * Audits note text that is already in memory.
 * @param {string} relativePath
 * @param {string} text
 * @returns {NoteReport}
 */
export function reportNoteText(relativePath, text) {
	const parsed = parseMarkers(text);
	const units = parsed.blocks.map((block) => unitReport(parsed, block.begin, 'block', blockBodyHash(parsed.lines, block), parsed.lines.slice(block.begin.line + 1, block.endLine)));
	if (parsed.note !== undefined) {
		units.unshift(unitReport(parsed, parsed.note, 'note', noteBodyHash(parsed.lines, parsed.note.line), parsed.lines.slice(parsed.note.line + 1)));
	}
	return { relativePath, authorship: authorshipOf(parsed), noteSystem: noteSystemOf(parsed), units, problems: parsed.problems };
}

/**
 * Units a human still has to look at (unverified or stale), oldest first,
 * excluding agent material and day logs.
 * @param {NoteReport[]} reports
 * @returns {QueueEntry[]}
 */
export function reviewQueue(reports) {
	return reports
		.filter((report) => !isQueueExcluded(report.relativePath))
		.flatMap((report) => report.units.filter((unit) => unit.verification !== 'verified').map((unit) => ({ relativePath: report.relativePath, unit, noteSystem: report.noteSystem })))
		.sort((left, right) => compareDates(left.unit.date, right.unit.date) || left.relativePath.localeCompare(right.relativePath) || left.unit.line - right.unit.line);
}

/**
 * Renders the index note listing every note with AI content.
 * @param {NoteReport[]} reports
 * @param {{ generatedAt: string, refreshCommand: string }} options
 */
export function renderIndex(reports, options) {
	const rows = reports
		.filter((report) => report.authorship !== 'human')
		.map((report) => {
			const latest = latestUnit(report.units);
			const verified = report.units.filter((unit) => unit.verification === 'verified').length;
			return `| [[${report.relativePath.replace(/\.md$/, '')}]] | ${report.authorship} | ${latest?.model ?? UNKNOWN_FIELD} | ${latest?.date ?? UNKNOWN_FIELD} | ${report.units.length} | ${summarizeStatus(report.units)} | ${verified}/${report.units.length} | ${summarizeVerification(report.units)} |`;
		});
	const problems = reports.flatMap((report) => report.problems.map((problem) => `- \`${report.relativePath}\`: ${problem}`));
	return [
		`%% generated by obsidian-ai-provenance on ${options.generatedAt} — do not edit by hand; refresh with \`${options.refreshCommand}\` %%`,
		'# AI provenance',
		'',
		'Notes listed here contain model-written content; notes not listed are human-written. Markers are `%% ai:… %%` comments and only show in editing view. Integrity `revised` means the text changed after the author wrote it. Verification `verified` means a human checked the unit as it stands, `stale` that it changed since, `unverified` that nobody has. Open units sit in [[Review queue]].',
		'',
		'| Note | Authorship | Last author | Last date | Units | Integrity | Verified | Verification |',
		'| --- | --- | --- | --- | --- | --- | --- | --- |',
		...(rows.length > 0 ? rows : ['| _none_ | | | | | | | |']),
		...(problems.length > 0 ? ['', '## Problems', '', ...problems] : []),
		'',
	].join('\n');
}

/**
 * Renders the review-queue note: one list item per open unit with inline
 * fields Dataview can read through `file.lists`.
 * @param {NoteReport[]} reports
 * @param {{ generatedAt: string, refreshCommand: string }} options
 */
export function renderReviewQueue(reports, options) {
	const units = reports.flatMap((report) => report.units);
	const queue = reviewQueue(reports);
	const items = queue.map(({ relativePath, unit, noteSystem }) => {
		const notePath = relativePath.replace(/\.md$/, '');
		const title = path.posix.basename(notePath);
		const target = unit.heading === undefined ? notePath : `${notePath}#${sanitizeHeadingLink(unit.heading)}`;
		const label = unit.heading === undefined ? title : `${title} · ${unit.heading}`;
		const fields = [
			`[state:: ${unit.verification}]`,
			`[model:: ${unit.model}]`,
			`[written:: ${unit.date}]`,
			...(unit.verifiedDate === undefined ? [] : [`[verified:: ${unit.verifiedDate}]`]),
			`[words:: ${unit.words}]`,
			`[unit:: ${unit.currentHash}]`,
			...(isUncheckedNoteSystem(noteSystem) ? [`[note_system:: ${noteSystem}]`] : []),
		];
		return `- [[${target}|${label}]] ${fields.join(' ')}`;
	});
	return [
		'---',
		`generated: ${options.generatedAt}`,
		`ai_units: ${units.length}`,
		`ai_verified: ${units.filter((unit) => unit.verification === 'verified').length}`,
		`ai_unverified: ${units.filter((unit) => unit.verification === 'unverified').length}`,
		`ai_stale: ${units.filter((unit) => unit.verification === 'stale').length}`,
		'---',
		`%% generated by obsidian-ai-provenance on ${options.generatedAt} — do not edit by hand; refresh with \`${options.refreshCommand}\` %%`,
		'# Review queue',
		'',
		'Model-written units nobody has verified yet, oldest first. Open the unit, check it against the instance, the code or the source, resolve any `#verify` tag, then run **Mark passage as verified** (AI provenance markers plugin). `stale` units were verified once and changed since. `note_system` below 1.0 marks a migrated note whose alignment nobody has checked yet; its claims carry lower weight until the note is bumped or the unit verified. Agent material under `_agent/` and day logs are not listed.',
		'',
		...(items.length > 0 ? items : ['- _nothing to review_']),
		'',
	].join('\n');
}

// ---------------------------------------------------------------- vault files

/**
 * Walks up from a path to the nearest directory containing `.obsidian`.
 * @param {string} startPath file or directory path
 * @returns {string | undefined}
 */
export function findVaultRoot(startPath) {
	let currentDir = path.resolve(startPath);
	while (true) {
		if (existsSync(path.join(currentDir, '.obsidian'))) return currentDir;
		const parentDir = path.dirname(currentDir);
		if (parentDir === currentDir) return undefined;
		currentDir = parentDir;
	}
}

/**
 * True when the note sits in a folder whose lines carry their own attribution
 * (see STAMP_EXEMPT_SEGMENTS); the hook gates such notes but does not stamp them.
 * @param {string} filePath
 * @param {string} vaultRoot
 */
export function isStampExempt(filePath, vaultRoot) {
	const segments = path.relative(path.resolve(vaultRoot), path.resolve(filePath)).split(path.sep);
	return segments.slice(0, -1).some((segment) => STAMP_EXEMPT_SEGMENTS.has(segment));
}

/**
 * A v1 vault declares itself with a `Workspace.md` manifest at its root; the
 * strict edit gate applies there.
 * @param {string} vaultRoot
 */
export function isV1Vault(vaultRoot) {
	return existsSync(path.join(vaultRoot, WORKSPACE_NOTE_NAME));
}

/**
 * Nearest folder at or above the note, up to the vault root, that holds a
 * `Workspace.md`: Vaultday's deepest-match rule, so a workspace carved out of a
 * vault subfolder gets the strict gate too. Undefined when none declares one.
 * @param {string} filePath
 * @param {string} vaultRoot
 * @returns {string | undefined}
 */
export function findWorkspaceRoot(filePath, vaultRoot) {
	const root = path.resolve(vaultRoot);
	let currentDir = path.dirname(path.resolve(filePath));
	while (currentDir.startsWith(root)) {
		if (existsSync(path.join(currentDir, WORKSPACE_NOTE_NAME))) return currentDir;
		if (currentDir === root) break;
		currentDir = path.dirname(currentDir);
	}
	return undefined;
}

/**
 * Returns the vault root when the file is a content note the convention
 * applies to (a `.md` file inside a vault, outside skipped folders and not an
 * agent-rules or generated file); otherwise undefined.
 * @param {string} filePath
 * @returns {string | undefined}
 */
export function resolveVaultNote(filePath) {
	if (path.extname(filePath) !== '.md') return undefined;
	if (SKIPPED_BASENAMES.has(path.basename(filePath))) return undefined;
	const vaultRoot = findVaultRoot(path.dirname(filePath));
	if (vaultRoot === undefined) return undefined;
	const segments = path.relative(vaultRoot, path.resolve(filePath)).split(path.sep);
	if (segments.slice(0, -1).some((segment) => SKIPPED_DIRECTORY_NAMES.has(segment))) return undefined;
	return vaultRoot;
}

/**
 * Lists every content note in a vault (same exclusions as resolveVaultNote).
 * @param {string} vaultRoot
 * @returns {string[]} absolute paths, sorted
 */
export function listVaultNotes(vaultRoot) {
	/** @type {string[]} */
	const notes = [];
	walk(vaultRoot);
	return notes.sort();

	/** @param {string} dir */
	function walk(dir) {
		for (const entry of readdirSync(dir, { withFileTypes: true })) {
			const entryPath = path.join(dir, entry.name);
			if (entry.isDirectory()) {
				if (!SKIPPED_DIRECTORY_NAMES.has(entry.name)) walk(entryPath);
			} else if (entry.isFile() && entry.name.endsWith('.md') && !SKIPPED_BASENAMES.has(entry.name)) {
				notes.push(entryPath);
			}
		}
	}
}

/**
 * Local calendar date as YYYY-MM-DD.
 * @param {Date} [date]
 */
export function formatLocalDate(date = new Date()) {
	const year = date.getFullYear();
	const month = String(date.getMonth() + 1).padStart(2, '0');
	const day = String(date.getDate()).padStart(2, '0');
	return `${year}-${month}-${day}`;
}

/** @param {string} text */
export function countNewlines(text) {
	let count = 0;
	for (const character of text) if (character === '\n') count++;
	return count;
}

// ---------------------------------------------------------------- helpers: parsing

/** @param {string} line */
function stripCarriageReturn(line) {
	return line.endsWith('\r') ? line.slice(0, -1) : line;
}

/** @param {string[]} lines */
function findBodyStart(lines) {
	if (lines[0] !== '---') return 0;
	for (let index = 1; index < lines.length; index++) {
		if (FRONTMATTER_DELIMITERS.has(lines[index] ?? '')) return index + 1;
	}
	return 0;
}

/**
 * @param {string[]} lines
 * @param {number} from
 */
function findFirstNonBlankLine(lines, from) {
	for (let index = from; index < lines.length; index++) {
		if ((lines[index] ?? '').trim().length > 0) return index;
	}
	return -1;
}

/** @param {string[]} lines */
function trimBlankEdges(lines) {
	let start = 0;
	let end = lines.length;
	while (start < end && (lines[start] ?? '').length === 0) start++;
	while (end > start && (lines[end - 1] ?? '').length === 0) end--;
	return lines.slice(start, end);
}

/**
 * Nearest heading text above a line, or undefined.
 * @param {string[]} lines
 * @param {number} fromLine
 * @param {number} bodyStart
 */
function headingAbove(lines, fromLine, bodyStart) {
	for (let index = fromLine - 1; index >= bodyStart; index--) {
		const match = HEADING_PATTERN.exec(lines[index] ?? '');
		if (match !== null) return match[2];
	}
	return undefined;
}

/**
 * The note system version declared in the frontmatter, if any.
 * @param {ParsedNote} parsed
 */
function noteSystemOf(parsed) {
	for (const line of parsed.lines.slice(0, parsed.bodyStart)) {
		const match = NOTE_SYSTEM_FRONTMATTER_PATTERN.exec(stripCarriageReturn(line));
		if (match !== null) return match[1]?.replace(/^["']|["']$/g, '');
	}
	return undefined;
}

/**
 * A declared version below 1.0 means the note was migrated and its alignment is unchecked.
 * @param {string | undefined} noteSystem
 */
export function isUncheckedNoteSystem(noteSystem) {
	if (noteSystem === undefined) return false;
	const version = Number.parseFloat(noteSystem);
	return Number.isFinite(version) && version < 1;
}

/** @param {string} heading */
function sanitizeHeadingLink(heading) {
	return heading.replace(/[[\]|#]/g, '').trim();
}

// ---------------------------------------------------------------- helpers: stamping

/**
 * @param {Marker} marker
 * @param {number} lastLine
 * @param {AffectedLines} affected
 */
function isUnitAffected(marker, lastLine, affected) {
	if (affected === 'all' || marker.isBare) return true;
	return affected.some((range) => range.start <= lastLine && range.end >= marker.line);
}

/**
 * An author-only bare marker names who wrote the text; a stamped marker being
 * refreshed takes the current session's author.
 * @param {Marker} marker
 * @param {string} sessionAuthor
 */
function authorForStamp(marker, sessionAuthor) {
	return marker.isBare && marker.model !== undefined ? marker.model : sessionAuthor;
}

/**
 * @param {Marker} marker
 * @returns {Verification | undefined}
 */
function verificationOfMarker(marker) {
	return marker.verifiedDate !== undefined && marker.verifiedHash !== undefined ? { date: marker.verifiedDate, hash: marker.verifiedHash } : undefined;
}

// ---------------------------------------------------------------- helpers: auditing

/**
 * @param {ParsedNote} parsed
 * @param {Marker} marker
 * @param {'note' | 'block'} kind
 * @param {string} currentHash
 * @param {string[]} contentLines
 * @returns {UnitReport}
 */
function unitReport(parsed, marker, kind, currentHash, contentLines) {
	const model = marker.model ?? UNKNOWN_FIELD;
	return {
		kind,
		line: marker.line + 1,
		model,
		authorKind: authorKindOf(model),
		date: marker.date ?? UNKNOWN_FIELD,
		status: unitStatus(marker, currentHash),
		currentHash,
		verification: verificationOf(marker, currentHash),
		verifiedDate: marker.verifiedDate,
		words: contentWordCount(normalizeLines(contentLines, marker.prefix)),
		heading: kind === 'block' ? headingAbove(parsed.lines, marker.line, parsed.bodyStart) : undefined,
	};
}

/**
 * @param {Marker} marker
 * @param {string} currentHash
 * @returns {UnitStatus}
 */
function unitStatus(marker, currentHash) {
	if (marker.isBare) return 'unhashed';
	return marker.hash === currentHash ? 'intact' : 'revised';
}

/**
 * The most recently dated unit; bare (unhashed) units have no date and only count when nothing else does.
 * @param {UnitReport[]} units
 */
function latestUnit(units) {
	const datedUnits = units.filter((unit) => unit.date !== UNKNOWN_FIELD);
	const candidates = datedUnits.length > 0 ? datedUnits : units;
	return candidates.reduce((latest, unit) => (latest === undefined || unit.date > latest.date ? unit : latest), /** @type {UnitReport | undefined} */ (undefined));
}

/** @param {UnitReport[]} units */
function summarizeStatus(units) {
	const revised = units.filter((unit) => unit.status === 'revised').length;
	const unhashed = units.filter((unit) => unit.status === 'unhashed').length;
	if (revised > 0) return `revised ${revised}/${units.length}`;
	if (unhashed > 0) return `unhashed ${unhashed}/${units.length}`;
	return 'intact';
}

/** @param {UnitReport[]} units */
function summarizeVerification(units) {
	const stale = units.filter((unit) => unit.verification === 'stale').length;
	const unverified = units.filter((unit) => unit.verification === 'unverified').length;
	if (stale > 0) return `stale ${stale}`;
	if (unverified > 0) return `unverified ${unverified}`;
	return 'verified';
}

/** @param {string} relativePath */
function isQueueExcluded(relativePath) {
	return relativePath.split('/').slice(0, -1).some((segment) => QUEUE_EXCLUDED_SEGMENTS.has(segment));
}

/**
 * Unknown dates sort last.
 * @param {string} left
 * @param {string} right
 */
function compareDates(left, right) {
	if (left === right) return 0;
	if (left === UNKNOWN_FIELD) return 1;
	if (right === UNKNOWN_FIELD) return -1;
	return left < right ? -1 : 1;
}

// ---------------------------------------------------------------- helpers: gating

/** @param {string} detail */
function proseDenialReason(detail) {
	return [
		`This edit ${detail}.`,
		'Wrap the inserted text in bare provenance markers, each on its own line:',
		'%% ai:begin %%',
		'…inserted text…',
		'%% ai:end %%',
		'The provenance hook fills in author, date and fingerprint. Name a foreign author with `%% ai:begin <model-id> %%`. Edits inside an existing ai unit pass without extra markers.',
	].join('\n');
}

/**
 * True when a marker line present in oldString is missing from newString.
 * @param {string} oldString
 * @param {string} newString
 */
function removesMarkerLines(oldString, newString) {
	const newLines = new Set(newString.split('\n').map(stripCarriageReturn));
	return oldString.split('\n').map(stripCarriageReturn).some((line) => isMarkerLine(line) && !newLines.has(line));
}

/**
 * True when newString carries a verified marker line that oldString did not.
 * @param {string} oldString
 * @param {string} newString
 */
function addsVerification(oldString, newString) {
	const oldLines = new Set(oldString.split('\n').map(stripCarriageReturn));
	return verificationLines(newString).some((line) => !oldLines.has(line));
}

/** @param {string} line */
function isMarkerLine(line) {
	return MARKER_PATTERN.test(line) || END_PATTERN.test(line);
}

/**
 * 0-based line index where oldString starts in fileText, or undefined.
 * @param {string} fileText
 * @param {string} oldString
 */
function locateLine(fileText, oldString) {
	if (oldString.length === 0) return undefined;
	const offset = fileText.indexOf(oldString);
	if (offset === -1) return undefined;
	return countNewlines(fileText.slice(0, offset));
}

/**
 * Lines that newString adds over oldString and that sit outside ai markers
 * (multiset line diff, order-insensitive). Marker lines themselves are skipped.
 * @param {string} oldString
 * @param {string} newString
 */
function unmarkedAddedLines(oldString, newString) {
	const remainingOldLines = lineMultiset(oldString);
	let isInsideUnit = false;
	/** @type {string[]} */
	const added = [];
	for (const line of newString.split('\n').map(stripCarriageReturn)) {
		if (parseMarkerLine(line, 0) !== undefined) {
			isInsideUnit = true;
			continue;
		}
		if (END_PATTERN.test(line)) {
			isInsideUnit = false;
			continue;
		}
		if (isInsideUnit) continue;
		const remaining = remainingOldLines.get(line) ?? 0;
		if (remaining > 0) {
			remainingOldLines.set(line, remaining - 1);
			continue;
		}
		added.push(line);
	}
	return added;
}

/**
 * Lines of oldString that newString no longer contains (multiset).
 * @param {string} oldString
 * @param {string} newString
 */
function removedLines(oldString, newString) {
	const remainingNewLines = lineMultiset(newString);
	/** @type {string[]} */
	const removed = [];
	for (const line of oldString.split('\n').map(stripCarriageReturn)) {
		const remaining = remainingNewLines.get(line) ?? 0;
		if (remaining > 0) remainingNewLines.set(line, remaining - 1);
		else removed.push(line);
	}
	return removed;
}

/** @param {string} text */
function lineMultiset(text) {
	/** @type {Map<string, number>} */
	const counts = new Map();
	for (const line of text.split('\n').map(stripCarriageReturn)) counts.set(line, (counts.get(line) ?? 0) + 1);
	return counts;
}

/** @param {string[]} lines */
function wordCount(lines) {
	return lines.reduce((total, line) => total + line.split(/\s+/).filter((word) => word.length > 0).length, 0);
}

/**
 * Words of a unit's content; tokens without a letter or digit (list dashes, bare punctuation) do not count.
 * @param {string} text
 */
function contentWordCount(text) {
	return words(text).filter((word) => /[\p{L}\p{N}]/u.test(word)).length;
}

/** @param {string} line */
function words(line) {
	return line.split(/\s+/).filter((word) => word.length > 0);
}

/** @param {string} line */
function isStructuredEntry(line) {
	return TABLE_ROW_PATTERN.test(line) || HEADING_LINE_PATTERN.test(line) || DATED_ITEM_PATTERN.test(line);
}

/**
 * @param {string} before
 * @param {string} after
 */
function isCheckboxToggle(before, after) {
	if (!CHECKBOX_PATTERN.test(before) || !CHECKBOX_PATTERN.test(after)) return false;
	return before.replace(CHECKBOX_PATTERN, '$1 ]') === after.replace(CHECKBOX_PATTERN, '$1 ]');
}

/**
 * The lines agree once inline fields are removed, or both are whole-line fields with the same key.
 * @param {string} before
 * @param {string} after
 */
function isInlineFieldChange(before, after) {
	if (stripInlineFields(before) === stripInlineFields(after)) return true;
	const beforeKey = WHOLE_LINE_FIELD_PATTERN.exec(before)?.[1];
	const afterKey = WHOLE_LINE_FIELD_PATTERN.exec(after)?.[1];
	return beforeKey !== undefined && beforeKey === afterKey;
}

/** @param {string} line */
function stripInlineFields(line) {
	return line.replace(INLINE_FIELD_PATTERN, ' ').replace(/\s+/g, ' ').trim();
}

/**
 * Every differing token is a `#tag` or a `[[link]]`.
 * @param {string} before
 * @param {string} after
 */
function isTagOrLinkTouchUp(before, after) {
	const differing = symmetricDifference(words(before), words(after));
	return differing.length > 0 && differing.every((token) => TAG_TOKEN_PATTERN.test(token) || LINK_TOKEN_PATTERN.test(token));
}

/**
 * @param {string[]} left
 * @param {string[]} right
 */
function symmetricDifference(left, right) {
	const leftCounts = new Map();
	for (const token of left) leftCounts.set(token, (leftCounts.get(token) ?? 0) + 1);
	/** @type {string[]} */
	const onlyRight = [];
	for (const token of right) {
		const remaining = leftCounts.get(token) ?? 0;
		if (remaining > 0) leftCounts.set(token, remaining - 1);
		else onlyRight.push(token);
	}
	const onlyLeft = [...leftCounts.entries()].flatMap(([token, count]) => Array(count).fill(token));
	return [...onlyLeft, ...onlyRight];
}

/**
 * Words that have to change to turn `before` into `after` (longest common subsequence).
 * @param {string} before
 * @param {string} after
 */
function wordDistance(before, after) {
	const left = words(before);
	const right = words(after);
	/** @type {number[]} */
	let previous = Array(right.length + 1).fill(0);
	for (const token of left) {
		/** @type {number[]} */
		const current = [0];
		for (let column = 1; column <= right.length; column++) {
			current[column] = token === right[column - 1] ? (previous[column - 1] ?? 0) + 1 : Math.max(previous[column] ?? 0, current[column - 1] ?? 0);
		}
		previous = current;
	}
	const common = previous[right.length] ?? 0;
	return Math.max(left.length, right.length) - common;
}

/**
 * Removes and returns the pool line most similar to `line` (shared prefix and suffix length), or undefined.
 * @param {string[]} pool mutated
 * @param {string} line
 */
function takeMostSimilar(pool, line) {
	let bestIndex = -1;
	let bestScore = -1;
	pool.forEach((candidate, index) => {
		const score = sharedPrefixLength(candidate, line) + sharedSuffixLength(candidate, line);
		if (score > bestScore) {
			bestScore = score;
			bestIndex = index;
		}
	});
	if (bestIndex === -1) return undefined;
	return pool.splice(bestIndex, 1)[0];
}

/**
 * @param {string} left
 * @param {string} right
 */
function sharedPrefixLength(left, right) {
	let length = 0;
	while (length < left.length && length < right.length && left[length] === right[length]) length++;
	return length;
}

/**
 * @param {string} left
 * @param {string} right
 */
function sharedSuffixLength(left, right) {
	let length = 0;
	while (length < left.length && length < right.length && left[left.length - 1 - length] === right[right.length - 1 - length]) length++;
	return length;
}
