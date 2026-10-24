import { readFileSync } from 'node:fs';
import path from 'node:path';
import { addDays, type IsoDate, type PlaygroundCalendar } from './calendar.ts';
import { type DayLog, doneMoments, renderDayLogs } from './day-log.ts';
import type { WorkspaceBuild } from './mock-studio.ts';
import {
	type PlannedProject,
	type PlannedTask,
	renderProjectNote,
	renderTaskLines,
} from './notes.ts';
import { holdNoteToCutoff } from './vault-text.ts';

export const PERSONAL_FOLDER = 'Personal';

const LOG_FOLDER = 'Log';

// Task and log offsets count from today, so due dates read "tomorrow" or "in a week" whenever the playground is reset.

const PROJECTS: PlannedProject[] = [
	{
		title: 'Home renovation',
		summary:
			'Bathroom first, then the hallway. Budget and quotes live in the notes below the tasks.',
		nextSteps: [
			{
				id: 'task-1',
				text: 'Measure the hallway for the new floor',
				created: -9,
				completed: -3,
			},
			{
				id: 'task-2',
				text: 'Get three quotes for the bathroom tiles',
				created: -6,
				due: 6,
			},
			{
				id: 'task-3',
				text: 'Order paint samples',
				created: -2,
				scheduled: 0,
				due: 1,
				estimate: '20m',
			},
			{
				id: 'task-4',
				text: 'Book the electrician for the hallway lights',
				created: -2,
				scheduled: 3,
			},
		],
		pendingInput: [
			{
				id: 'task-5',
				text: 'Quote from the tiler',
				created: -5,
				assignedTo: 'Alex',
			},
		],
	},
	{
		title: 'Half marathon',
		summary: 'Race day is in five weeks; three runs a week until then.',
		nextSteps: [
			{
				id: 'task-6',
				text: 'Interval session, 6 × 800 m',
				created: -4,
				completed: -2,
			},
			{ id: 'task-7', text: 'Long run, 16 km', created: -1, scheduled: 1 },
			{
				id: 'task-8',
				text: 'Buy new running shoes',
				created: -8,
				scheduled: 0,
				due: 4,
			},
			{ id: 'task-9', text: 'Register for the race', created: -10, due: 9 },
		],
		pendingInput: [
			{
				id: 'task-10',
				text: 'Training plan tweaks from the coach',
				created: -3,
				assignedTo: 'Jordan',
			},
		],
	},
];

const ADMIN_TASKS: PlannedTask[] = [
	{
		id: 'task-11',
		text: 'Cancel the old phone contract',
		created: -12,
		scheduled: -2,
		due: -1,
	},
	{
		id: 'task-12',
		text: 'File the tax return',
		created: -14,
		scheduled: 2,
		due: 5,
		estimate: '2h',
	},
	{ id: 'task-13', text: 'Renew the passport', created: -3, due: 30 },
];

const GROCERIES: PlannedTask[] = [
	{ id: 'task-14', text: 'Coffee beans', created: -1 },
	{ id: 'task-15', text: 'Lemons', created: 0 },
];

/**
 * The life workspace: two projects with due and scheduled tasks, an admin
 * list with one overdue task, the leave note every workspace reads its days
 * off from, and the day logs of yesterday and today.
 *
 * @param fixtureRoot `core/fixtures/two-workspaces`, whose life root this derives from.
 */
export function buildPersonal(
	fixtureRoot: string,
	calendar: PlaygroundCalendar,
): WorkspaceBuild {
	const dateOf = (offset: number) => addDays(calendar.today, offset);
	const days = personalDayLogs(dateOf);
	const cutoff = {
		today: calendar.today,
		now: calendar.now,
		doneTimes: doneMoments(days),
	};
	const settle = (text: string) => holdNoteToCutoff(text, cutoff);
	return {
		folderName: PERSONAL_FOLDER,
		files: [
			{
				path: '.vaultday/settings.json',
				content: personalSettings(fixtureRoot),
			},
			{ path: 'Workspace.md', content: workspaceNote() },
			{ path: 'Home.md', content: homeNote() },
			{ path: 'Leave.md', content: leaveNote(calendar.focusMonday) },
			{
				path: 'Notes/Admin.md',
				content: settle(
					listNote('Admin', renderTaskLines(ADMIN_TASKS, dateOf)),
				),
			},
			{
				path: 'Notes/Groceries.md',
				content: settle(groceriesNote(fixtureRoot, dateOf)),
			},
			...PROJECTS.map((project) => ({
				path: `Projects/${project.title}/${project.title}.md`,
				content: settle(renderProjectNote(project, dateOf)),
			})),
			...renderDayLogs(days, {
				logFolder: LOG_FOLDER,
				today: calendar.today,
				now: calendar.now,
				firstSessionNumber: 1,
			}),
		],
	};
}

/**
 * The leave note: the fixture's layout, with a long weekend on the previous
 * week's Friday, an autumn break three weeks out, a public holiday next week
 * and the year's last holidays.
 */
export function leaveNote(focusMonday: IsoDate): string {
	const year = Number(focusMonday.slice(0, 4));
	const holidayYear = focusMonday > `${year}-12-26` ? year + 1 : year;
	return [
		'# Leave',
		'',
		'Synthetic dates for the playground. One date or range per line, ISO form, a label after it.',
		'',
		'## Leave',
		`- ${addDays(focusMonday, -3)} Long weekend`,
		`- ${addDays(focusMonday, 21)}..${addDays(focusMonday, 24)} Autumn break`,
		`- ${holidayYear}-12-28..${holidayYear}-12-31 Winter break`,
		'',
		'## Public holidays',
		`- ${addDays(focusMonday, 9)} Synthetic holiday`,
		`- ${holidayYear}-12-25 Christmas Day`,
		`- ${holidayYear}-12-26 Boxing Day`,
		'',
	].join('\n');
}

function personalSettings(fixtureRoot: string): string {
	const fixtureSettings: unknown = JSON.parse(
		readFileSync(path.join(fixtureRoot, '.vaultday/settings.json'), 'utf8'),
	);
	if (typeof fixtureSettings !== 'object' || fixtureSettings === null)
		throw new Error('the two-workspaces fixture holds no settings object');
	// "amber" is no workspace color of this build; green is the life sphere's own.
	return `${JSON.stringify({ ...fixtureSettings, name: PERSONAL_FOLDER, kind: 'life', accent: 'green', ...Object.fromEntries([['log_folder', LOG_FOLDER]]) }, undefined, 2)}\n`;
}

function groceriesNote(
	fixtureRoot: string,
	dateOf: (offset: number) => IsoDate,
): string {
	const fixtureText = readFileSync(
		path.join(fixtureRoot, 'Notes/Groceries.md'),
		'utf8',
	).trimEnd();
	return [fixtureText, ...renderTaskLines(GROCERIES, dateOf), ''].join('\n');
}

function listNote(title: string, taskLines: string[]): string {
	return [
		'---',
		'type: note',
		'note_system: "1.0"',
		'---',
		`# ${title}`,
		'',
		...taskLines,
		'',
	].join('\n');
}

function workspaceNote(): string {
	return [
		'---',
		'type: workspace',
		'note_system: "1.0"',
		'---',
		'# Personal',
		'',
		'Read by the provenance hook and Dataview only. The app never reads this note; `.vaultday/settings.json` is the manifest.',
		'',
	].join('\n');
}

function homeNote(): string {
	return [
		'---',
		'type: home',
		'---',
		'# Home',
		'',
		...PROJECTS.map(
			(project) =>
				`- [[Projects/${project.title}/${project.title}|${project.title}]]`,
		),
		'- [[Notes/Admin|Admin]]',
		'- [[Leave]]',
		'',
	].join('\n');
}

function personalDayLogs(dateOf: (offset: number) => IsoDate): DayLog[] {
	const renovation = { project: 'Home renovation' };
	const marathon = { project: 'Half marathon' };
	return [
		{
			date: dateOf(-1),
			entries: [
				{
					kind: 'done',
					time: '07:15',
					text: 'Easy run, 8 km',
					subject: marathon,
				},
				{
					kind: 'note',
					time: '12:30',
					text: 'Tiler can come by next week for the quote',
					subject: renovation,
					taskId: 'task-5',
				},
				{
					kind: 'decision',
					time: '19:40',
					text: 'Matte white tiles for the bathroom walls',
					subject: renovation,
				},
			],
		},
		{
			date: dateOf(0),
			entries: [
				{
					kind: 'status',
					time: '08:10',
					text: 'Long run moved to tomorrow; rest day today',
					subject: marathon,
					taskId: 'task-7',
				},
				{
					kind: 'session',
					start: '10:00',
					minutes: 45,
					text: 'Compare tile quotes',
					subject: renovation,
					taskId: 'task-2',
				},
			],
		},
	];
}
