import {
	addTask,
	appendLog,
	formatTaskLine,
	nextTaskId,
	report,
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
import { ok, type Result } from './result.ts';

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
 * The report bundle for a story number or a client over a date range:
 * frontmatter, task state, log lines and an authorship summary computed with
 * the provenance core from the markers and `[agent:: …]` fields present.
 */
export function runReport(
	workspace: WorkspaceHandle,
	command: ReportCommand,
): CommandResult {
	const built = callCore(() =>
		report(workspace, command.subject, command.from, command.to),
	);
	if (!built.ok) return built;
	const { stories, log, ...header } = built.value;
	const authorship: AuthorshipSummary = summariseAuthorship(
		stories.map((story) => ({
			relativePath: story.relativePath,
			text: story.noteText,
		})),
		log.map((line) => line.agent ?? undefined),
	);
	const bundle = {
		...header,
		stories: stories.map(({ noteText: _noteText, frontmatter, ...story }) => ({
			...story,
			frontmatter: Object.fromEntries(
				frontmatter.map((entry) => [entry.key, entry.values]),
			),
		})),
		log,
		authorship,
	};
	return ok({ value: bundle, text: JSON.stringify(bundle, undefined, 2) });
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
