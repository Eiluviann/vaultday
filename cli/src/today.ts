import { openWorkspace, type TodayView, today } from '@vaultday/core-node';
import { err, ok, type Result } from './result.ts';

export type TodayOptions = {
	readonly workspacePath: string;
	readonly date?: string | undefined;
};

export type TodayCall = {
	readonly view: TodayView;
	/** Wall time of the `today()` call through the binding, in milliseconds. */
	readonly callMilliseconds: number;
};

/**
 * Opens the workspace and builds its Today view through the native binding.
 * A folder without a manifest, a refused manifest, an unreadable vault and a
 * malformed date all come back as a failed `Result` with the core's message.
 */
export function loadToday(options: TodayOptions): Result<TodayCall> {
	try {
		const workspace = openWorkspace(options.workspacePath);
		const start = performance.now();
		const view = today(workspace, options.date);
		return ok({ view, callMilliseconds: performance.now() - start });
	} catch (error) {
		return err(error instanceof Error ? error : new Error(String(error)));
	}
}

/** Renders the view as the lines `vaultday today` prints. */
export function renderToday(view: TodayView): string {
	const openTaskCount = view.stories.reduce(
		(count, story) =>
			count + story.nextSteps.length + story.pendingInput.length,
		0,
	);
	const header = [
		`${view.workspaceName} · Today ${view.date}`,
		`${openTaskCount} open tasks in ${view.stories.length} stories`,
		'',
	];
	const storyBlocks = view.stories.flatMap((story) => [
		`## ${[story.number, story.title].filter((part) => part !== undefined && part !== null).join(' · ')}`,
		...story.nextSteps.map((task) => renderTask(task, 'next')),
		...story.pendingInput.map((task) => renderTask(task, 'pending')),
		'',
	]);
	const logLines =
		view.log.length === 0
			? ['(no log lines)']
			: view.log.map(
					(line) =>
						`- ${line.time} ${line.text}${line.kind ? ` [${line.kind}]` : ''}`,
				);
	return [...header, ...storyBlocks, `## Log ${view.date}`, ...logLines].join(
		'\n',
	);
}

type TaskLike = TodayView['stories'][number]['nextSteps'][number];

function renderTask(task: TaskLike, section: 'next' | 'pending'): string {
	const indent = '\t'.repeat(task.depth);
	const marker = task.state === 'in-progress' ? '[/]' : '[ ]';
	const tags = [
		section === 'pending' && task.assignedTo
			? `waiting on ${task.assignedTo}`
			: undefined,
		task.estimate,
		task.due ? `due ${task.due}` : undefined,
	].filter((tag) => tag !== undefined);
	const suffix = tags.length === 0 ? '' : `  (${tags.join(', ')})`;
	return `${indent}- ${marker} ${task.text}${suffix}`;
}
