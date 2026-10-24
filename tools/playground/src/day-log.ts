import {
	addMinutes,
	type ClockTime,
	type IsoDate,
	minutesOfDay,
} from './calendar.ts';
import type { DoneMoment } from './vault-text.ts';

/** What a log line is about: a story by number, or a project hub by title. */
export type LogSubject = { story: string } | { project: string };

/** A timer session; the line is written when it stops, as the app's writer does. */
export type SessionEntry = {
	kind: 'session';
	start: ClockTime;
	minutes: number;
	/** The task's text, as the writer copies it. */
	text: string;
	subject: LogSubject;
	taskId?: string;
};

/** Any other day-log line. */
export type NoteEntry = {
	kind: 'done' | 'decision' | 'blocker' | 'status' | 'note';
	time: ClockTime;
	text: string;
	subject?: LogSubject;
	taskId?: string;
	/** The model that wrote the line, for a model-written line. */
	agent?: string;
};

export type LogEntry = SessionEntry | NoteEntry;

/** One day's log as planned; `renderDayLogs` decides which lines exist yet. */
export type DayLog = {
	date: IsoDate;
	entries: LogEntry[];
};

/** A generated file, relative to its workspace root. */
export type PlaygroundFile = {
	path: string;
	content: string;
};

/** When the logs are rendered, and the folder they go to. */
export type LogRenderOptions = {
	logFolder: string;
	today: IsoDate;
	now: ClockTime;
	/** The number of the first session id (`ses-<n>`); later sessions count up in date order. */
	firstSessionNumber: number;
};

type RenderedLine = {
	time: ClockTime;
	text: string;
};

/**
 * Renders the day logs as the vault convention writes them: a day after
 * today has no log, today's holds only the lines before `now`, and a day left
 * with no line gets no file.
 *
 * @throws When two sessions of one day overlap: the plan itself is wrong.
 */
export function renderDayLogs(
	days: DayLog[],
	options: LogRenderOptions,
): PlaygroundFile[] {
	const sortedDays = [...days]
		.filter((day) => day.date <= options.today)
		.sort((left, right) => left.date.localeCompare(right.date));
	let nextSessionNumber = options.firstSessionNumber;
	const files: PlaygroundFile[] = [];
	for (const day of sortedDays) {
		assertSessionsDoNotOverlap(day);
		const lines = day.entries
			.map((entry) => {
				if (entry.kind !== 'session') return renderNoteEntry(entry);
				const line = renderSessionEntry(entry, `ses-${nextSessionNumber}`);
				nextSessionNumber += 1;
				return line;
			})
			.filter((line) => day.date < options.today || line.time <= options.now)
			.sort((left, right) => left.time.localeCompare(right.time));
		if (lines.length === 0) continue;
		files.push({
			path: `${options.logFolder}/${day.date}.md`,
			content: dayLogNote(day.date, lines),
		});
	}
	return files;
}

/** `90` as `1h 30m`, `60` as `1h`, `45` as `45m`: the durations the writer puts on session lines. */
export function formatDuration(minutes: number): string {
	const hours = Math.floor(minutes / 60);
	const rest = minutes % 60;
	if (hours === 0) return `${rest}m`;
	return rest === 0 ? `${hours}h` : `${hours}h ${rest}m`;
}

/** The day and time each task's done line is written at, by task id; the last one wins. */
export function doneMoments(days: DayLog[]): Map<string, DoneMoment> {
	return new Map(
		days.flatMap((day) =>
			day.entries.flatMap((entry) =>
				entry.kind === 'done' && entry.taskId !== undefined
					? [[entry.taskId, { date: day.date, time: entry.time }] as const]
					: [],
			),
		),
	);
}

/** The time a session's line carries: the moment it stopped. */
export function sessionEnd(entry: SessionEntry): ClockTime {
	return addMinutes(entry.start, entry.minutes);
}

function renderSessionEntry(
	entry: SessionEntry,
	sessionId: string,
): RenderedLine {
	const time = sessionEnd(entry);
	const fields = [
		subjectField(entry.subject),
		'[kind:: session]',
		`[id:: ${sessionId}]`,
		taskField(entry.taskId),
	];
	return {
		time,
		text: `- ${time} session · ${entry.text} · ${formatDuration(entry.minutes)} ${joinFields(fields)}`,
	};
}

function renderNoteEntry(entry: NoteEntry): RenderedLine {
	const fields = [
		subjectField(entry.subject),
		`[kind:: ${entry.kind}]`,
		taskField(entry.taskId),
		entry.agent === undefined ? undefined : `[agent:: ${entry.agent}]`,
	];
	return {
		time: entry.time,
		text: `- ${entry.time} ${entry.kind} · ${entry.text} ${joinFields(fields)}`,
	};
}

function dayLogNote(date: IsoDate, lines: RenderedLine[]): string {
	return [
		'---',
		'type: log',
		`date: ${date}`,
		'---',
		`# ${date}`,
		'',
		'## Log',
		...lines.map((line) => line.text),
		'',
	].join('\n');
}

function assertSessionsDoNotOverlap(day: DayLog): void {
	const sessions = day.entries
		.filter((entry): entry is SessionEntry => entry.kind === 'session')
		.sort((left, right) => left.start.localeCompare(right.start));
	sessions.forEach((session, index) => {
		const next = sessions[index + 1];
		if (
			next !== undefined &&
			minutesOfDay(sessionEnd(session)) > minutesOfDay(next.start)
		) {
			throw new Error(
				`${day.date}: the session at ${session.start} runs into the one at ${next.start}`,
			);
		}
	});
}

function subjectField(subject: LogSubject | undefined): string | undefined {
	if (subject === undefined) return undefined;
	return 'story' in subject
		? `[story:: ${subject.story}]`
		: `[project:: ${subject.project}]`;
}

function taskField(taskId: string | undefined): string | undefined {
	return taskId === undefined ? undefined : `[task:: ${taskId}]`;
}

function joinFields(fields: (string | undefined)[]): string {
	return fields.filter((field) => field !== undefined).join(' ');
}
