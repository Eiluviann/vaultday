import type { IsoDate } from './calendar.ts';

/** Turns a day offset of the plan into a date; each workspace picks its own anchor. */
export type DateOfOffset = (offset: number) => IsoDate;

/** One task line as planned; dates are day offsets from the workspace's anchor. */
export type PlannedTask = {
	id: string;
	text: string;
	created: number;
	completed?: number;
	isInProgress?: boolean;
	scheduled?: number;
	due?: number;
	estimate?: string;
	assignedTo?: string;
	subtasks?: PlannedTask[];
	/** Indented note lines under the task. */
	details?: string[];
};

/** A story note under a project hub, as the tracker sync would leave it. */
export type PlannedStory = {
	number: string;
	title: string;
	projectFolder: string;
	sourceId: string;
	status: 'draft' | 'ready' | 'in-progress' | 'on-hold' | 'done';
	priority: 'low' | 'medium' | 'high';
	due?: number;
	estimate?: string;
	sortIndex: number;
	summary: string;
	nextSteps: PlannedTask[];
	pendingInput: PlannedTask[];
	/** Dated note lines: `[offset, text]`. */
	notes?: [number, string][];
};

/** A life project note: no ticket, its own Next Steps and Pending Input. */
export type PlannedProject = {
	title: string;
	summary: string;
	nextSteps: PlannedTask[];
	pendingInput: PlannedTask[];
};

/** The tracker a work workspace's stories come from. */
export type StorySource = {
	host: string;
	ticketUrl: (ticket: string) => string;
};

/** The story note, frontmatter first, in the layout of the fixture stories. */
export function renderStoryNote(
	story: PlannedStory,
	source: StorySource,
	dateOf: DateOfOffset,
): string {
	const frontmatter = [
		'type: story',
		`number: ${story.number}`,
		`source_host: ${source.host}`,
		'source_kind: issue',
		`source_id: "${story.sourceId}"`,
		`status: ${story.status}`,
		`priority: ${story.priority}`,
		`due: ${story.due === undefined ? '' : dateOf(story.due)}`.trimEnd(),
		`estimate: ${story.estimate ?? ''}`.trimEnd(),
		`sort_index: ${story.sortIndex}`,
		'note_system: "1.0"',
	];
	const notes = story.notes ?? [];
	return [
		'---',
		...frontmatter,
		'---',
		`# ${story.title}`,
		'',
		`${story.summary} Ticket: [${story.number}](${source.ticketUrl(story.number)}).`,
		'',
		'## Next Steps',
		...renderTaskLines(story.nextSteps, dateOf),
		'',
		'## Pending Input',
		...renderTaskLines(story.pendingInput, dateOf),
		...(notes.length === 0
			? []
			: [
					'',
					'## Notes',
					...notes.map(([offset, text]) => `- ${dateOf(offset)} ${text}`),
				]),
		'',
		'## Activity',
		'```dataview',
		`TABLE WITHOUT ID time, text FROM "Log" WHERE story = "${story.number}"`,
		'```',
		'',
	].join('\n');
}

/** A life project note. */
export function renderProjectNote(
	project: PlannedProject,
	dateOf: DateOfOffset,
): string {
	return [
		'---',
		'type: project',
		'status: active',
		'note_system: "1.0"',
		'---',
		`# ${project.title}`,
		'',
		project.summary,
		'',
		'## Next Steps',
		...renderTaskLines(project.nextSteps, dateOf),
		'',
		'## Pending Input',
		...renderTaskLines(project.pendingInput, dateOf),
		'',
	].join('\n');
}

/** Task lines with their fields in the convention's order, subtasks indented by a tab. */
export function renderTaskLines(
	tasks: PlannedTask[],
	dateOf: DateOfOffset,
	depth = 0,
): string[] {
	const indent = '\t'.repeat(depth);
	return tasks.flatMap((task) => [
		`${indent}- [${taskMark(task)}] ${task.text}  ${taskFields(task, dateOf).join(' ')}`,
		...(task.details ?? []).map((detail) => `${indent}\t- ${detail}`),
		...renderTaskLines(task.subtasks ?? [], dateOf, depth + 1),
	]);
}

function taskMark(task: PlannedTask): string {
	if (task.completed !== undefined) return 'x';
	return task.isInProgress === true ? '/' : ' ';
}

function taskFields(task: PlannedTask, dateOf: DateOfOffset): string[] {
	const fields: [string, string | undefined][] = [
		['id', task.id],
		['created', dateOf(task.created)],
		[
			'scheduled',
			task.scheduled === undefined ? undefined : dateOf(task.scheduled),
		],
		['due', task.due === undefined ? undefined : dateOf(task.due)],
		['estimate', task.estimate],
		['assigned_to', task.assignedTo],
		[
			'completion',
			task.completed === undefined ? undefined : dateOf(task.completed),
		],
	];
	return fields.flatMap(([key, value]) =>
		value === undefined ? [] : [`[${key}:: ${value}]`],
	);
}
