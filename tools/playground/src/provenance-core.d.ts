// The vendored provenance engine is plain JavaScript with JSDoc; these are the parts the playground uses.
declare module '@vaultday/provenance-core' {
	export type UnitReport = {
		kind: 'note' | 'block';
		/** 1-based line of the marker. */
		line: number;
		status: 'intact' | 'revised' | 'unhashed';
		currentHash: string;
	};

	export type NoteReport = {
		relativePath: string;
		authorship: 'ai' | 'mixed' | 'human';
		units: UnitReport[];
		problems: string[];
	};

	export function reportNoteText(
		relativePath: string,
		text: string,
	): NoteReport;
}
