import { z } from 'zod';
import {
	type ApiConnection,
	type ApiMethod,
	callApi,
	readApiConnection,
} from './api-client.ts';
import type { CommandResult } from './commands.ts';
import type { Failure } from './failure.ts';
import type { Environment } from './provenance.ts';
import { err, ok, type Result } from './result.ts';

export const STATUS_FORMATS = ['text', 'json', 'sketchybar'] as const;
export type StatusFormat = (typeof STATUS_FORMATS)[number];

/** What `vaultday timer start` asks for: a task id or text the app matches, optionally backdated. */
export type TimerStartCommand = {
	readonly target: string;
	readonly minutesAgo?: number | undefined;
};

const TIMER_SCHEMA = z.object({
	sessionId: z.string(),
	taskId: z.string().nullish(),
	story: z.string().nullish(),
	text: z.string(),
	isPaused: z.boolean(),
	startedAt: z.string(),
	elapsedSeconds: z.number().int().nonnegative(),
});

const STATUS_SCHEMA = z.object({
	workspace: z.string().nullish(),
	timer: TIMER_SCHEMA.nullish(),
	day: z.object({
		date: z.string(),
		spentSeconds: z.number().int().nonnegative(),
		loggedSeconds: z.number().int().nonnegative(),
		workloadSeconds: z.number().int().nonnegative(),
	}),
	logLine: z.string().nullish(),
});

/** The app's answer to every timer route and to `GET /v1/status`. */
export type StatusPayload = z.infer<typeof STATUS_SCHEMA>;
export type TimerPayload = z.infer<typeof TIMER_SCHEMA>;

const SECONDS_PER_MINUTE = 60;
const SECONDS_PER_HOUR = 3600;

/** `vaultday status`: the running timer and the day's logged time. */
export async function runStatus(
	environment: Environment,
	format: StatusFormat,
): Promise<CommandResult> {
	const status = await requestStatus(environment, 'GET', '/v1/status');
	if (!status.ok) return status;
	const text =
		format === 'sketchybar'
			? renderSketchybar(status.value)
			: renderStatus(status.value);
	return ok({ value: status.value, text });
}

/** `vaultday timer start <task-id|text>`. */
export async function runTimerStart(
	environment: Environment,
	command: TimerStartCommand,
): Promise<CommandResult> {
	const body =
		command.minutesAgo === undefined
			? { target: command.target }
			: { target: command.target, minutesAgo: command.minutesAgo };
	const status = await requestStatus(
		environment,
		'POST',
		'/v1/timer/start',
		body,
	);
	return status.ok ? timerOutput('Started', status.value) : status;
}

/** `vaultday timer stop`: the app stops the session and writes its day-log line. */
export async function runTimerStop(
	environment: Environment,
): Promise<CommandResult> {
	const status = await requestStatus(environment, 'POST', '/v1/timer/stop');
	if (!status.ok) return status;
	const logLine = status.value.logLine;
	const text = logLine
		? `Stopped · logged\n${logLine}`
		: 'Stopped · the day log already holds this session';
	return ok({ value: status.value, text });
}

/** `vaultday timer continue`: a new session on the last timer's task. */
export async function runTimerContinue(
	environment: Environment,
): Promise<CommandResult> {
	const status = await requestStatus(environment, 'POST', '/v1/timer/continue');
	return status.ok ? timerOutput('Continued', status.value) : status;
}

/** The status as a person reads it: the timer line, then the day's time. */
export function renderStatus(status: StatusPayload): string {
	const timerLine = status.timer
		? [
				status.timer.isPaused ? 'Paused' : 'Running',
				clockText(status.timer.elapsedSeconds),
				...timerSubject(status.timer),
			].join(' · ')
		: 'No timer running';
	return [timerLine, dayLine(status)].join('\n');
}

/**
 * The status as `key=value` lines for a sketchybar item script: one key per
 * line, values on one line, nothing quoted, so the script can read them with
 * `IFS='=' read -r key value` and never evaluates app text as shell.
 */
export function renderSketchybar(status: StatusPayload): string {
	const timer = status.timer;
	const state = timer ? (timer.isPaused ? 'paused' : 'running') : 'idle';
	const elapsed = timer ? clockText(timer.elapsedSeconds) : '';
	const label = timer
		? [elapsed, timerSubject(timer).join(' · ')].join(' ')
		: '';
	const lines = [
		['state', state],
		['elapsed', elapsed],
		['task', timer?.taskId ?? ''],
		['story', timer?.story ?? ''],
		['text', timer?.text ?? ''],
		['label', label],
		['logged', compactDurationText(status.day.loggedSeconds)],
		['workload', compactDurationText(status.day.workloadSeconds)],
		[
			'ring',
			`${compactDurationText(status.day.loggedSeconds)} / ${compactDurationText(status.day.workloadSeconds)}`,
		],
		['workspace', status.workspace ?? ''],
	];
	return lines
		.map(([key, value]) => `${key}=${oneLine(value ?? '')}`)
		.join('\n');
}

/** `0:47:12`: hours, then minutes and seconds in two digits. */
export function clockText(totalSeconds: number): string {
	const hours = Math.floor(totalSeconds / SECONDS_PER_HOUR);
	const minutes = Math.floor(
		(totalSeconds % SECONDS_PER_HOUR) / SECONDS_PER_MINUTE,
	);
	const seconds = totalSeconds % SECONDS_PER_MINUTE;
	return `${hours}:${twoDigits(minutes)}:${twoDigits(seconds)}`;
}

async function requestStatus(
	environment: Environment,
	method: ApiMethod,
	route: string,
	body?: unknown,
): Promise<Result<StatusPayload, Failure>> {
	const connection = await readApiConnection(environment);
	if (!connection.ok) return connection;
	return parseStatus(await call(connection.value, method, route, body));
}

function call(
	connection: ApiConnection,
	method: ApiMethod,
	route: string,
	body: unknown,
): Promise<Result<unknown, Failure>> {
	return body === undefined
		? callApi(connection, method, route)
		: callApi(connection, method, route, body);
}

function parseStatus(
	answer: Result<unknown, Failure>,
): Result<StatusPayload, Failure> {
	if (!answer.ok) return answer;
	const parsed = STATUS_SCHEMA.safeParse(answer.value);
	return parsed.success
		? ok(parsed.data)
		: err({
				kind: 'internal',
				message: `Vaultday answered in an unexpected shape: ${parsed.error.message}`,
			});
}

function timerOutput(verb: string, status: StatusPayload): CommandResult {
	const subject = status.timer ? timerSubject(status.timer).join(' · ') : '';
	return ok({ value: status, text: `${verb} · ${subject}` });
}

function timerSubject(timer: TimerPayload): string[] {
	return [timer.taskId, timer.text].filter(
		(part): part is string => typeof part === 'string' && part !== '',
	);
}

function dayLine(status: StatusPayload): string {
	const { loggedSeconds, spentSeconds, workloadSeconds } = status.day;
	return `Logged ${durationText(loggedSeconds)} of ${durationText(workloadSeconds)} today · tracked ${durationText(spentSeconds)}`;
}

/** `3h 10m`, `45m`, `8h`. */
function durationText(totalSeconds: number): string {
	const hours = Math.floor(totalSeconds / SECONDS_PER_HOUR);
	const minutes = Math.floor(
		(totalSeconds % SECONDS_PER_HOUR) / SECONDS_PER_MINUTE,
	);
	if (hours === 0) return `${minutes}m`;
	return minutes === 0 ? `${hours}h` : `${hours}h ${minutes}m`;
}

/** `3h10`, `45m`, `8h`: the bar's short form. */
function compactDurationText(totalSeconds: number): string {
	const hours = Math.floor(totalSeconds / SECONDS_PER_HOUR);
	const minutes = Math.floor(
		(totalSeconds % SECONDS_PER_HOUR) / SECONDS_PER_MINUTE,
	);
	if (hours === 0) return `${minutes}m`;
	return minutes === 0 ? `${hours}h` : `${hours}h${twoDigits(minutes)}`;
}

function twoDigits(value: number): string {
	return String(value).padStart(2, '0');
}

function oneLine(text: string): string {
	return text.replace(/[\r\n]+/g, ' ').trim();
}
