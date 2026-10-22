import { classifyLines, type RegionLine } from './markdown-regions.ts';

/** What kind of migration edit touched a line; drives the fingerprint-refresh eligibility. */
export type EditKind =
	| 'frontmatter'
	| 'task'
	| 'link'
	| 'query'
	| 'structure'
	| 'fingerprint';

export type EditableLine = {
	text: string;
	/** The line's text before migration; undefined for an inserted line. */
	original: string | undefined;
	edits: EditKind[];
};

/** A note loaded for in-memory migration; lines keep their pre-migration text for unit comparisons. */
export type NoteDocument = {
	relativePath: string;
	originalText: string;
	lines: EditableLine[];
};

export function openDocument(relativePath: string, text: string): NoteDocument {
	return {
		relativePath,
		originalText: text,
		lines: text
			.split('\n')
			.map((line) => ({ text: line, original: line, edits: [] })),
	};
}

export function renderDocument(document: NoteDocument): string {
	return document.lines.map((line) => line.text).join('\n');
}

export function isDocumentChanged(document: NoteDocument): boolean {
	return renderDocument(document) !== document.originalText;
}

export function currentTexts(document: NoteDocument): string[] {
	return document.lines.map((line) => line.text);
}

export function currentRegions(document: NoteDocument): RegionLine[] {
	return classifyLines(currentTexts(document));
}

/** Replaces one line's text in place and records the edit kind (no-op when the text is unchanged). */
export function replaceLineText(
	document: NoteDocument,
	index: number,
	text: string,
	kind: EditKind,
): void {
	const line = document.lines[index];
	if (line === undefined)
		throw new Error(`line ${index} does not exist in ${document.relativePath}`);
	if (line.text === text) return;
	line.text = text;
	if (!line.edits.includes(kind)) line.edits.push(kind);
}

/** Inserts new lines before `index`; they carry no original text. */
export function insertLines(
	document: NoteDocument,
	index: number,
	texts: string[],
	kind: EditKind,
): void {
	if (index < 0 || index > document.lines.length)
		throw new Error(`cannot insert at ${index} in ${document.relativePath}`);
	document.lines.splice(
		index,
		0,
		...texts.map((text) => ({ text, original: undefined, edits: [kind] })),
	);
}

/** Replaces the lines in [start, end) with new lines (used for the frontmatter block). */
export function replaceLineRange(
	document: NoteDocument,
	start: number,
	end: number,
	texts: string[],
	kind: EditKind,
): void {
	const previous = document.lines.slice(start, end).map((line) => line.text);
	if (
		previous.length === texts.length &&
		previous.every((text, offset) => text === texts[offset])
	)
		return;
	document.lines.splice(
		start,
		end - start,
		...texts.map((text) => ({ text, original: undefined, edits: [kind] })),
	);
}

export function countEditsByKind(
	document: NoteDocument,
): Map<EditKind, number> {
	const counts = new Map<EditKind, number>();
	for (const line of document.lines) {
		for (const kind of line.edits)
			counts.set(kind, (counts.get(kind) ?? 0) + 1);
	}
	return counts;
}
