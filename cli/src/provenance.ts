import {
	formatLocalDate,
	formatMarker,
	hashText,
	type NoteReport,
	normalizeLines,
	reportNoteText,
} from '@vaultday/provenance-core';

export type Environment = Readonly<Record<string, string | undefined>>;

/** Whether the text of a command was written by a model, and which one. */
export type ModelTextPolicy =
	| { readonly isModelText: false }
	| { readonly isModelText: true; readonly author: string };

export type AuthorshipSummary = {
	/** Notes by authorship, from the `%% ai:… %%` markers present. */
	readonly notes: {
		readonly ai: number;
		readonly mixed: number;
		readonly human: number;
	};
	/** Words inside model-text units across the reported notes. */
	readonly modelWords: number;
	/** Per note: authorship and the model-text units found. */
	readonly noteReports: readonly NoteReport[];
	/** Day-log lines: those with an `[agent:: …]` field were written by that model. */
	readonly logLines: {
		readonly total: number;
		readonly byModel: number;
		readonly byFact: number;
		readonly models: Readonly<Record<string, number>>;
	};
};

const AUTHOR_ENVIRONMENT_KEY = 'AI_PROVENANCE_AUTHOR';
const FALLBACK_ENVIRONMENT_KEY = 'ANTHROPIC_MODEL';
const FALLBACK_AUTHOR = 'claude-unknown';

/**
 * Decides who wrote the text. `--ai` / `--human` (`explicit`) win; otherwise
 * `AI_PROVENANCE_AUTHOR`, the variable the vault's provenance hook reads, marks
 * a session as model-driven. The author is that variable, else
 * `ANTHROPIC_MODEL`, else `claude-unknown`, with whitespace and `·` folded to `-`
 * like the hook does.
 */
export function modelTextPolicy(
	explicit: boolean | undefined,
	environment: Environment,
): ModelTextPolicy {
	const declaredAuthor = environment[AUTHOR_ENVIRONMENT_KEY]?.trim();
	const isModelText = explicit ?? (declaredAuthor ?? '').length > 0;
	if (!isModelText) return { isModelText: false };
	const rawAuthor =
		declaredAuthor ||
		environment[FALLBACK_ENVIRONMENT_KEY]?.trim() ||
		FALLBACK_AUTHOR;
	return { isModelText: true, author: rawAuthor.replace(/[\s·]+/g, '-') };
}

/** The `%% ai:begin … %%` line that opens a block holding exactly `line`. */
export function beginMarkerFor(line: string, author: string): string {
	return formatMarker(
		'begin',
		'',
		author,
		formatLocalDate(),
		hashText(normalizeLines([line])),
	);
}

export { formatLocalDate };

/** Summarises authorship from the notes' markers and the day-log lines' `[agent:: …]` fields. */
export function summariseAuthorship(
	notes: readonly { readonly relativePath: string; readonly text: string }[],
	logAgents: readonly (string | undefined)[],
): AuthorshipSummary {
	const noteReports = notes.map((note) =>
		reportNoteText(note.relativePath, note.text),
	);
	const countNotes = (authorship: NoteReport['authorship']): number =>
		noteReports.filter((report) => report.authorship === authorship).length;
	const models: Record<string, number> = {};
	for (const agent of logAgents) {
		if (agent !== undefined) models[agent] = (models[agent] ?? 0) + 1;
	}
	const byModel = logAgents.filter((agent) => agent !== undefined).length;
	return {
		notes: {
			ai: countNotes('ai'),
			mixed: countNotes('mixed'),
			human: countNotes('human'),
		},
		modelWords: noteReports
			.flatMap((report) => report.units)
			.reduce((words, unit) => words + unit.words, 0),
		noteReports,
		logLines: {
			total: logAgents.length,
			byModel,
			byFact: logAgents.length - byModel,
			models,
		},
	};
}
