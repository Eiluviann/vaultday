import {
	addTask,
	appendLog,
	formatTaskLine,
	nextTaskId,
	report,
	reportBundleJson,
	tickTaskDone,
	today,
	type WorkspaceHandle,
} from '@vaultday/core-node';
import { callCore } from './core.ts';
import type { Failure } from './failure.ts';
import {
	type AuthorshipSummary,
	beginMarkerFor,
	formatLocalDate,
	type ModelTextPolicy,
	summariseAuthorship,
} from './provenance.ts';
import { renderToday } from './render.ts';
import { err, ok, type Result } from './result.ts';

export const LOG_KINDS = [
	'session',
	'done',
	'decision',
	'blocker',
	'status',
	'note',
] as const;
export type LogKind = (typeof LOG_KINDS)[number];

export const TASK_SECTIONS = ['next', 'pending'] as const;
export type TaskSectionName = (typeof TASK_SECTIONS)[number];

/** What a command produced: `value` is the JSON form, `text` what a person reads. */
export type CommandOutput = {
	readonly value: unknown;
	readonly text: string;
};

export type CommandResult = Result<CommandOutput, Failure>;

export type LogCommand = {
	readonly kind: LogKind;
	readonly text: string;
	readonly story?: string | undefined;
	readonly task?: string | undefined;
	readonly date?: string | undefined;
	readonly time?: string | undefined;
	readonly textPolicy: ModelTextPolicy;
};

export type ReportCommand = {
	readonly subject: string;
	readonly from: string;
	readonly to: string;
};

export type TaskAddCommand = {
	readonly text: string;
	readonly story: string;
	readonly section: TaskSectionName;
	readonly assignedTo?: string | undefined;
	readonly textPolicy: ModelTextPolicy;
};

export function isLogKind(value: string): value is LogKind {
	return LOG_KINDS.some((kind) => kind === value);
}

export function isTaskSectionName(value: string): value is TaskSectionName {
	return TASK_SECTIONS.some((section) => section === value);
}

/** Today's stories with their open tasks, the day's log lines and who is waited on. */
export function runToday(
	workspace: WorkspaceHandle,
	date: string | undefined,
): CommandResult {
	const view = callCore(() => today(workspace, date));
	if (!view.ok) return view;
	return ok({ value: view.value, text: renderToday(view.value) });
}

/**
 * Appends a line to the day log. A model-written line names its author in
 * `[agent:: …]`, the attribution the provenance core accepts for log lines;
 * a fact line carries nothing.
 */
export function runLog(
	workspace: WorkspaceHandle,
	command: LogCommand,
): CommandResult {
	const change = callCore(() =>
		appendLog(workspace, {
			kind: command.kind,
			text: command.text,
			...present('story', command.story),
			...present('task', command.task),
			...present('date', command.date),
			...present('time', command.time),
			...(command.textPolicy.isModelText
				? { agent: command.textPolicy.author }
				: {}),
		}),
	);
	if (!change.ok) return change;
	const { relativePath, lineNumber, newLine } = change.value;
	return ok({
		value: change.value,
		text: `Logged in ${relativePath}:${lineNumber}\n${newLine}`,
	});
}

/** Adds a task to a story; a model-written task sits inside a provenance block. */
export function runTaskAdd(
	workspace: WorkspaceHandle,
	command: TaskAddCommand,
): CommandResult {
	const created = formatLocalDate();
	const added = callCore(() => {
		const id = nextTaskId(workspace);
		const beginMarker = command.textPolicy.isModelText
			? beginMarkerFor(
					formatTaskLine(command.text, id, created, command.assignedTo),
					command.textPolicy.author,
				)
			: undefined;
		return addTask(workspace, {
			storyNumber: command.story,
			section: command.section === 'next' ? 'next-steps' : 'pending-input',
			text: command.text,
			...present('assignedTo', command.assignedTo),
			id,
			created,
			...present('beginMarker', beginMarker),
		});
	});
	if (!added.ok) return added;
	const { id, change } = added.value;
	return ok({
		value: added.value,
		text: `Added ${id} in ${change.relativePath}:${change.lineNumber}\n${change.newLine}`,
	});
}

/** Ticks a task done by its id; exactly that line changes. */
export function runTaskDone(
	workspace: WorkspaceHandle,
	id: string,
): CommandResult {
	const change = callCore(() => tickTaskDone(workspace, id));
	if (!change.ok) return change;
	const { relativePath, lineNumber, newLine } = change.value;
	return ok({
		value: change.value,
		text: `Done in ${relativePath}:${lineNumber}\n${newLine}`,
	});
}

/**
 * The report bundle for a story number or a client over a date range. The
 * core assembles it (frontmatter, task state, log lines, sessions and hours by
 * billing model) and the app hands out the same content; the one part the core
 * leaves open is `authorship`, computed here with the provenance core from the
 * markers and `[agent:: …]` fields present.
 */
export function runReport(
	workspace: WorkspaceHandle,
	command: ReportCommand,
): CommandResult {
	const bundle = callCore(() =>
		reportBundleJson(workspace, command.subject, command.from, command.to),
	);
	if (!bundle.ok) return bundle;
	const sources = callCore(() =>
		report(workspace, command.subject, command.from, command.to),
	);
	if (!sources.ok) return sources;
	const parsed: unknown = JSON.parse(bundle.value);
	if (!isRecord(parsed)) {
		return err({
			kind: 'internal',
			message: 'the core returned a report that is not an object',
		});
	}
	const authorship: AuthorshipSummary = summariseAuthorship(
		sources.value.stories.map((story) => ({
			relativePath: story.relativePath,
			text: story.noteText,
		})),
		sources.value.log.map((line) => line.agent ?? undefined),
	);
	const filled = { ...parsed, authorship };
	return ok({ value: filled, text: JSON.stringify(filled, undefined, 2) });
}

function isRecord(value: unknown): value is Record<string, unknown> {
	return typeof value === 'object' && value !== null && !Array.isArray(value);
}

/** `{ key: value }`, or nothing when the value is absent: the binding's optional fields reject an explicit `undefined`. */
function present<K extends string>(
	key: K,
	value: string | undefined,
): Partial<Record<K, string>> {
	const field: Partial<Record<K, string>> = {};
	if (value !== undefined) field[key] = value;
	return field;
}
