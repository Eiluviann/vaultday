import { fileBirthDate } from './birthtimes.ts';
import {
	type RefreshOutcome,
	refreshFingerprints,
} from './fingerprint-refresh.ts';
import { splitFrontmatterBlock } from './frontmatter.ts';
import {
	type FrontmatterRuleContext,
	type NoteClass,
	transformFrontmatter,
} from './frontmatter-transform.ts';
import type { IdAllocator } from './id-allocator.ts';
import {
	buildLinkRewriteTable,
	type LinkRewriteTable,
	rewritePathStrings,
	rewriteWikilinks,
} from './link-rewrite.ts';
import {
	type ResolvedDelete,
	type ResolvedMove,
	resolveMoves,
} from './moves.ts';
import {
	countEditsByKind,
	type EditKind,
	isDocumentChanged,
	type NoteDocument,
	openDocument,
	renderDocument,
} from './note-document.ts';
import { PathMap } from './path-map.ts';
import {
	basenameWithoutExtension,
	isUnderDirectory,
	parentDirectory,
} from './paths.ts';
import type { MoveSpec, Profile } from './profile.ts';
import type { ProvenanceHasher } from './provenance-core.ts';
import { rewriteQueries } from './query-rewrite.ts';
import { planRenames, type RenameEntry } from './rename-plan.ts';
import { ok, type Result } from './result.ts';
import { transformTaskLines } from './task-line-transform.ts';
import {
	isFolderNote,
	isNotePath,
	isTaskIdExemptPath,
	isTemplatePath,
	readNoteText,
	scanVault,
	type VaultEntry,
} from './vault-scan.ts';

export type RuleName = 'frontmatter' | 'tasks' | 'rename' | 'queries' | 'moves';
export const ALL_RULES: readonly RuleName[] = [
	'frontmatter',
	'tasks',
	'rename',
	'queries',
	'moves',
];

export type BuildPlanOptions = {
	vaultRoot: string;
	profile: Profile;
	moveSpecs: MoveSpec[];
	rules: ReadonlySet<RuleName>;
	birthtimes: ReadonlyMap<string, string>;
	allocator: IdAllocator;
	hasher: ProvenanceHasher;
};

export type FileChange = {
	fromPath: string;
	/** Final vault-relative path; undefined when the note moves to another vault. */
	toPath: string | undefined;
	externalPath?: string;
	before: string;
	after: string;
	edits: Map<EditKind, number>;
	refreshes: RefreshOutcome[];
	noteClass: NoteClass;
};

export type MigrationPlan = {
	vaultRoot: string;
	changes: FileChange[];
	renames: RenameEntry[];
	moves: ResolvedMove[];
	deletes: ResolvedDelete[];
	allocatedIds: string[];
	hubs: Array<{ path: string; key: string }>;
	typeGuesses: Array<{ path: string; type: string; kind: string | undefined }>;
	reports: string[];
};

type LoadedNote = {
	entry: VaultEntry;
	document: NoteDocument;
	noteClass: NoteClass;
};

type PathChanges = {
	renames: RenameEntry[];
	moves: ResolvedMove[];
	deletes: ResolvedDelete[];
	pathMap: PathMap;
	linkTable: LinkRewriteTable;
};

/** Builds the whole migration in memory; nothing on disk changes. */
export function buildPlan(options: BuildPlanOptions): Result<MigrationPlan> {
	const entries = scanVault(options.vaultRoot);
	const notes = loadNotes(entries, options.profile);
	const pathChangesResult = planPathChanges(notes, entries, options);
	if (!pathChangesResult.ok) return pathChangesResult;
	const pathChanges = pathChangesResult.value;
	const plan: MigrationPlan = {
		vaultRoot: options.vaultRoot,
		changes: [],
		renames: pathChanges.renames,
		moves: pathChanges.moves,
		deletes: pathChanges.deletes,
		allocatedIds: [],
		hubs: [],
		typeGuesses: [],
		reports: [],
	};
	for (const note of notes) {
		if (isDeletedPath(note.entry.relativePath, pathChanges)) continue;
		const change = migrateNote(note, pathChanges, options, plan);
		if (change !== undefined) plan.changes.push(change);
	}
	plan.allocatedIds = [...options.allocator.allocated];
	return ok(plan);
}

export function isPlanEmpty(plan: MigrationPlan): boolean {
	return (
		plan.changes.length === 0 &&
		plan.renames.length === 0 &&
		plan.moves.length === 0 &&
		plan.deletes.length === 0
	);
}

function loadNotes(entries: VaultEntry[], profile: Profile): LoadedNote[] {
	return entries
		.filter(
			(entry) =>
				entry.kind === 'file' &&
				isNotePath(entry.relativePath) &&
				!profile.skipNotes.includes(entry.relativePath),
		)
		.map((entry) => {
			const document = openDocument(
				entry.relativePath,
				readNoteText(entry.absolutePath),
			);
			return {
				entry,
				document,
				noteClass: classifyNote(entry.relativePath, document, profile),
			};
		});
}

function planPathChanges(
	notes: LoadedNote[],
	entries: VaultEntry[],
	options: BuildPlanOptions,
): Result<PathChanges> {
	const stories = notes
		.filter((note) => note.noteClass === 'story')
		.map((note) => ({
			relativePath: note.entry.relativePath,
			number: storyNumber(note.document),
		}));
	const renamesResult = options.rules.has('rename')
		? planRenames(stories)
		: ok([]);
	if (!renamesResult.ok) return renamesResult;
	const pathMap = new PathMap();
	for (const rename of renamesResult.value) {
		pathMap.add({
			from: rename.fromDirectory,
			to: rename.toDirectory,
			kind: 'directory',
		});
		pathMap.add({
			from: `${rename.toDirectory}/${rename.oldTitle}.md`,
			to: rename.toNote,
			kind: 'file',
		});
	}
	const renamedEntries = entries.map((entry) => ({
		...entry,
		relativePath: pathMap.map(entry.relativePath) ?? entry.relativePath,
	}));
	const movesResult = options.rules.has('moves')
		? resolveMoves(options.moveSpecs, renamedEntries, options.vaultRoot)
		: ok({ moves: [], deletes: [] });
	if (!movesResult.ok) return movesResult;
	for (const move of movesResult.value.moves)
		pathMap.add({ from: move.from, to: move.to, kind: move.kind });
	const linkTable = buildLinkRewriteTable(
		pathMap,
		notes.map((note) => note.entry.relativePath),
	);
	return ok({
		renames: renamesResult.value,
		moves: movesResult.value.moves,
		deletes: movesResult.value.deletes,
		pathMap,
		linkTable,
	});
}

function migrateNote(
	note: LoadedNote,
	pathChanges: PathChanges,
	options: BuildPlanOptions,
	plan: MigrationPlan,
): FileChange | undefined {
	const { document, noteClass } = note;
	const relativePath = note.entry.relativePath;
	const finalPath = pathChanges.pathMap.map(relativePath);
	const renamedPath = renamedPathOf(relativePath, pathChanges.renames);
	const externalPath = pathChanges.moves.find(
		(move) => move.externalPath !== undefined && move.from === renamedPath,
	)?.externalPath;

	if (options.rules.has('frontmatter'))
		applyFrontmatterRules(note, pathChanges.renames, options, plan);
	if (options.rules.has('tasks')) {
		const isExempt =
			noteClass === 'template' || isTaskIdExemptPath(finalPath ?? relativePath);
		const outcome = transformTaskLines(document, {
			allocateId: isExempt
				? undefined
				: () => options.allocator.allocate('task'),
			shouldInsertNextSteps: noteClass === 'story',
		});
		plan.reports.push(...outcome.reports);
	}
	if (options.rules.has('queries'))
		rewriteQueries(document, pathChanges.pathMap);
	if (options.rules.has('rename') || options.rules.has('moves')) {
		rewriteWikilinks(document, pathChanges.linkTable);
		rewritePathStrings(
			document,
			pathChanges.pathMap,
			parentDirectory(finalPath ?? relativePath),
		);
	}
	const refreshes = refreshFingerprints(document, options.hasher);
	if (
		!isDocumentChanged(document) &&
		finalPath === relativePath &&
		externalPath === undefined
	)
		return undefined;
	return {
		fromPath: relativePath,
		toPath: finalPath,
		...(externalPath === undefined ? {} : { externalPath }),
		before: document.originalText,
		after: renderDocument(document),
		edits: countEditsByKind(document),
		refreshes,
		noteClass,
	};
}

function applyFrontmatterRules(
	note: LoadedNote,
	renames: RenameEntry[],
	options: BuildPlanOptions,
	plan: MigrationPlan,
): void {
	const relativePath = note.entry.relativePath;
	const context: FrontmatterRuleContext = {
		vaultKind: options.profile.vaultKind,
		noteClass: note.noteClass,
		birthDate: birthDateFor(note, options.birthtimes, plan.reports),
	};
	if (note.noteClass === 'hub') {
		const key = options.profile.hubs[basenameWithoutExtension(relativePath)];
		if (key === undefined)
			plan.reports.push(
				`${relativePath}: hub without a key in the profile; key not set`,
			);
		else {
			context.hub = { key, togglProject: options.profile.togglProject };
			plan.hubs.push({ path: relativePath, key });
		}
	}
	const guess = options.profile.typeGuesses.find(
		(candidate) => candidate.path === relativePath,
	);
	if (guess !== undefined) {
		context.guess = guess;
		plan.typeGuesses.push({
			path: relativePath,
			type: guess.type,
			kind: guess.kind,
		});
	}
	const rename = renames.find(
		(candidate) => candidate.fromNote === relativePath,
	);
	if (rename !== undefined) context.renamedFrom = rename.oldTitle;
	plan.reports.push(...transformFrontmatter(note.document, context).reports);
}

function birthDateFor(
	note: LoadedNote,
	birthtimes: ReadonlyMap<string, string>,
	reports: string[],
): string | undefined {
	if (note.noteClass === 'template') return undefined;
	const fromManifest = birthtimes.get(note.entry.relativePath);
	if (fromManifest !== undefined) return fromManifest;
	const fallback = fileBirthDate(note.entry.absolutePath);
	if (birthtimes.size > 0)
		reports.push(
			`${note.entry.relativePath}: not in the birthtime manifest; created taken from the copy's own birthtime (${fallback})`,
		);
	return fallback;
}

function classifyNote(
	relativePath: string,
	document: NoteDocument,
	profile: Profile,
): NoteClass {
	if (isTemplatePath(relativePath)) return 'template';
	const { frontmatter } = splitFrontmatterBlock(
		document.lines.map((line) => line.text),
	);
	if (frontmatter.has('Number') || frontmatter.has('number')) return 'story';
	const segments = relativePath.split('/');
	if (
		profile.vaultKind === 'work' &&
		segments.length === 3 &&
		segments[0] === 'Projects' &&
		isFolderNote(relativePath)
	)
		return 'hub';
	const isProject =
		frontmatter.list('tags').includes('project') ||
		frontmatter.scalar('type') === 'project';
	if (
		profile.vaultKind === 'personal' &&
		isFolderNote(relativePath) &&
		isProject
	)
		return 'personal-project';
	return 'other';
}

function storyNumber(document: NoteDocument): string {
	const { frontmatter } = splitFrontmatterBlock(
		document.lines.map((line) => line.text),
	);
	return frontmatter.scalar('Number') ?? frontmatter.scalar('number') ?? '';
}

function isDeletedPath(
	relativePath: string,
	pathChanges: PathChanges,
): boolean {
	const renamed = renamedPathOf(relativePath, pathChanges.renames);
	return pathChanges.deletes.some((entry) =>
		isUnderDirectory(renamed, entry.path),
	);
}

function renamedPathOf(relativePath: string, renames: RenameEntry[]): string {
	for (const rename of renames) {
		if (relativePath === rename.fromNote) return rename.toNote;
		if (relativePath.startsWith(`${rename.fromDirectory}/`))
			return `${rename.toDirectory}${relativePath.slice(rename.fromDirectory.length)}`;
	}
	return relativePath;
}
