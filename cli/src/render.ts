import type { TodayView } from '@vaultday/core-node';

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
	const waitingLines = view.waitingOn.map(
		(item) =>
			`- ${item.assignedTo}: ${item.text}${item.storyNumber ? ` (${item.storyNumber})` : ''}${item.ageDays === undefined || item.ageDays === null ? '' : `, ${item.ageDays} days`}`,
	);
	const waitingBlock =
		waitingLines.length === 0 ? [] : ['', '## Waiting on', ...waitingLines];
	return [
		...header,
		...storyBlocks,
		`## Log ${view.date}`,
		...logLines,
		...waitingBlock,
	].join('\n');
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
