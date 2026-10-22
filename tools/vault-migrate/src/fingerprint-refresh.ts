import type { EditKind, NoteDocument } from './note-document.ts';
import { currentTexts, replaceLineText } from './note-document.ts';
import type { ProvenanceHasher } from './provenance-core.ts';
import {
	type ProvenanceUnit,
	STAMPED_MARKER_HASH_PATTERN,
	scanUnits,
} from './provenance-units.ts';

/** Edits the migration makes mechanically; a unit changed only by these keeps its author and date and gets a new hash. */
export const MECHANICAL_EDIT_KINDS: ReadonlySet<EditKind> = new Set<EditKind>([
	'task',
	'link',
	'query',
	'fingerprint',
]);

export type RefreshOutcome = {
	/** 1-based line of the marker. */
	markerLine: number;
	kind: ProvenanceUnit['kind'];
	result: 'refreshed' | 'revised' | 'already-revised' | 'bare';
	/** Edit kinds found inside the unit. */
	edits: EditKind[];
	oldHash?: string;
	newHash?: string;
};

/**
 * For every unit whose body changed, rewrites the fingerprint when all changes
 * are mechanical and the unit was intact before; everything after the hash on
 * the marker line (a `verified` token, the closer) is kept byte for byte.
 */
export function refreshFingerprints(
	document: NoteDocument,
	hasher: ProvenanceHasher,
): RefreshOutcome[] {
	const texts = currentTexts(document);
	const outcomes: RefreshOutcome[] = [];
	// Blocks nested in an ai:note unit are refreshed first: the note's hash covers their marker lines.
	const units = scanUnits(texts).sort(
		(left, right) =>
			Number(left.kind === 'note') - Number(right.kind === 'note'),
	);
	for (const unit of units) {
		const bodyLines = document.lines.slice(unit.bodyStart, unit.bodyEnd + 1);
		const originalBody = bodyLines.flatMap((line) =>
			line.original === undefined ? [] : [line.original],
		);
		const currentBody = bodyLines.map((line) => line.text);
		const edits = [...new Set(bodyLines.flatMap((line) => line.edits))];
		const hasInsertedLines = bodyLines.some(
			(line) => line.original === undefined,
		);
		if (
			!hasInsertedLines &&
			originalBody.every((text, index) => text === currentBody[index])
		)
			continue;
		if (unit.hash === undefined) {
			outcomes.push({
				markerLine: unit.markerLine + 1,
				kind: unit.kind,
				result: 'bare',
				edits,
			});
			continue;
		}
		const originalHash = hasher.hashText(
			hasher.normalizeLines(originalBody, unit.prefix),
		);
		if (originalHash !== unit.hash) {
			outcomes.push({
				markerLine: unit.markerLine + 1,
				kind: unit.kind,
				result: 'already-revised',
				edits,
				oldHash: unit.hash,
			});
			continue;
		}
		if (
			hasInsertedLines ||
			edits.some((kind) => !MECHANICAL_EDIT_KINDS.has(kind))
		) {
			outcomes.push({
				markerLine: unit.markerLine + 1,
				kind: unit.kind,
				result: 'revised',
				edits,
				oldHash: unit.hash,
			});
			continue;
		}
		const newHash = hasher.hashText(
			hasher.normalizeLines(currentBody, unit.prefix),
		);
		const markerText = texts[unit.markerLine] ?? '';
		const match = STAMPED_MARKER_HASH_PATTERN.exec(
			markerText.replace(/\r$/, ''),
		);
		if (match === null) {
			outcomes.push({
				markerLine: unit.markerLine + 1,
				kind: unit.kind,
				result: 'revised',
				edits,
				oldHash: unit.hash,
			});
			continue;
		}
		const lineEnding = markerText.endsWith('\r') ? '\r' : '';
		replaceLineText(
			document,
			unit.markerLine,
			`${match[1] ?? ''}${newHash}${match[3] ?? ''}${lineEnding}`,
			'fingerprint',
		);
		outcomes.push({
			markerLine: unit.markerLine + 1,
			kind: unit.kind,
			result: 'refreshed',
			edits,
			oldHash: unit.hash,
			newHash,
		});
	}
	return outcomes;
}
