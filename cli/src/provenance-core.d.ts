// The vendored provenance engine is plain JavaScript with JSDoc; these are the parts the CLI uses.
declare module '@vaultday/provenance-core' {
	export type AuthorKind = 'model' | 'tool';

	export type UnitReport = {
		kind: 'note' | 'block';
		line: number;
		model: string;
		authorKind: AuthorKind;
		date: string;
		status: 'intact' | 'revised' | 'unhashed';
		verification: 'unverified' | 'verified' | 'stale';
		words: number;
	};

	export type NoteReport = {
		relativePath: string;
		authorship: 'ai' | 'mixed' | 'human';
		units: UnitReport[];
		problems: string[];
	};

	export function formatMarker(
		kind: 'note' | 'begin',
		prefix: string,
		author: string,
		date: string,
		hash: string,
	): string;
	export function hashText(normalizedText: string): string;
	export function normalizeLines(lines: string[], prefix?: string): string;
	export function formatLocalDate(date?: Date): string;
	export function reportNoteText(
		relativePath: string,
		text: string,
	): NoteReport;
}
