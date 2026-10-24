import { parseArgs } from 'node:util';
import {
	isLogKind,
	isTaskSectionName,
	LOG_KINDS,
	type LogKind,
	TASK_SECTIONS,
	type TaskSectionName,
} from './commands.ts';
import { type Failure, usageFailure } from './failure.ts';
import { err, ok, type Result } from './result.ts';
import { STATUS_FORMATS, type StatusFormat } from './timer-commands.ts';

export type ParsedCommand =
	| { readonly name: 'help'; readonly isRequested: boolean }
	| { readonly name: 'mcp' }
	| {
			readonly name: 'today';
			readonly startDirectory?: string | undefined;
			readonly date?: string | undefined;
			readonly shouldPrintJson: boolean;
	  }
	| {
			readonly name: 'log';
			readonly kind: LogKind;
			readonly text: string;
			readonly story?: string | undefined;
			readonly task?: string | undefined;
			readonly date?: string | undefined;
			readonly time?: string | undefined;
			readonly isModelText?: boolean | undefined;
			readonly shouldPrintJson: boolean;
	  }
	| {
			readonly name: 'report';
			readonly subject: string;
			readonly from: string;
			readonly to: string;
	  }
	| {
			readonly name: 'task-add';
			readonly text: string;
			readonly story: string;
			readonly section: TaskSectionName;
			readonly assignedTo?: string | undefined;
			readonly isModelText?: boolean | undefined;
			readonly shouldPrintJson: boolean;
	  }
	| {
			readonly name: 'task-done';
			readonly id: string;
			readonly shouldPrintJson: boolean;
	  }
	| {
			readonly name: 'timer-start';
			readonly target: string;
			readonly minutesAgo?: number | undefined;
			readonly shouldPrintJson: boolean;
	  }
	| {
			readonly name: 'timer-stop' | 'timer-continue';
			readonly shouldPrintJson: boolean;
	  }
	| {
			readonly name: 'status';
			readonly format: StatusFormat;
	  };

/** The commands that talk to the running app instead of the vault files. */
export type ApiCommand = Extract<
	ParsedCommand,
	{ name: 'timer-start' | 'timer-stop' | 'timer-continue' | 'status' }
>;

export const USAGE = `Usage:
  vaultday today [path] [--date YYYY-MM-DD] [--json]
  vaultday log <${LOG_KINDS.join('|')}> <text> [--story <number>] [--task <id>] [--date YYYY-MM-DD] [--time HH:MM] [--ai|--human] [--json]
  vaultday report <story number|client> --from YYYY-MM-DD --to YYYY-MM-DD
  vaultday task add <text> --story <number> [--section next|pending] [--assigned-to <who>] [--ai|--human] [--json]
  vaultday task done <id> [--json]
  vaultday timer start <task-id|text> [--ago <minutes>] [--json]
  vaultday timer stop [--json]
  vaultday timer continue [--json]
  vaultday status [--format text|json|sketchybar] [--json]
  vaultday mcp

The workspace is the nearest .vaultday/ at or above the working directory.
timer and status talk to the running Vaultday app over its localhost API.
Text a model wrote (--ai, or AI_PROVENANCE_AUTHOR set) is marked as model text.`;

const FLAGS = {
	date: { type: 'string' },
	time: { type: 'string' },
	story: { type: 'string' },
	task: { type: 'string' },
	from: { type: 'string' },
	to: { type: 'string' },
	section: { type: 'string' },
	'assigned-to': { type: 'string' },
	ai: { type: 'boolean' },
	human: { type: 'boolean' },
	json: { type: 'boolean' },
	format: { type: 'string' },
	ago: { type: 'string' },
	help: { type: 'boolean', short: 'h' },
} as const;

/** Parses `argv` after the executable and script. */
export function parseArguments(
	argv: readonly string[],
): Result<ParsedCommand, Failure> {
	try {
		return interpret(argv);
	} catch (thrown) {
		return err(
			usageFailure(thrown instanceof Error ? thrown.message : String(thrown)),
		);
	}
}

function interpret(argv: readonly string[]): Result<ParsedCommand, Failure> {
	const { values, positionals } = parseArgs({
		args: [...argv],
		options: FLAGS,
		allowPositionals: true,
		strict: true,
	});
	const [command, ...rest] = positionals;
	if (values.help || command === undefined) {
		return ok({ name: 'help', isRequested: values.help === true });
	}
	const shouldPrintJson = values.json === true;
	if (values.ai === true && values.human === true) {
		return err(usageFailure('--ai and --human exclude each other'));
	}
	const isModelText = values.ai ? true : values.human ? false : undefined;
	switch (command) {
		case 'mcp':
			return ok({ name: 'mcp' });
		case 'today':
			return ok({
				name: 'today',
				startDirectory: rest[0],
				date: values.date,
				shouldPrintJson,
			});
		case 'log':
			return parseLog(rest, values, isModelText, shouldPrintJson);
		case 'report': {
			const [subject] = rest;
			if (subject === undefined || !values.from || !values.to) {
				return err(usageFailure('report needs a subject, --from and --to'));
			}
			return ok({ name: 'report', subject, from: values.from, to: values.to });
		}
		case 'task':
			return parseTask(rest, values, isModelText, shouldPrintJson);
		case 'timer':
			return parseTimer(rest, values, shouldPrintJson);
		case 'status':
			return parseStatus(values, shouldPrintJson);
		default:
			return err(usageFailure(`unknown command ${command}`));
	}
}

type Flags = ReturnType<typeof parseArgs<{ options: typeof FLAGS }>>['values'];

function parseLog(
	rest: readonly string[],
	values: Flags,
	isModelText: boolean | undefined,
	shouldPrintJson: boolean,
): Result<ParsedCommand, Failure> {
	const [kind, ...words] = rest;
	if (kind === undefined || !isLogKind(kind)) {
		return err(usageFailure(`log kind must be one of ${LOG_KINDS.join(', ')}`));
	}
	if (words.length === 0) return err(usageFailure('log needs a text'));
	return ok({
		name: 'log',
		kind,
		text: words.join(' '),
		story: values.story,
		task: values.task,
		date: values.date,
		time: values.time,
		isModelText,
		shouldPrintJson,
	});
}

function parseTask(
	rest: readonly string[],
	values: Flags,
	isModelText: boolean | undefined,
	shouldPrintJson: boolean,
): Result<ParsedCommand, Failure> {
	const [action, ...words] = rest;
	if (action === 'done') {
		const [id] = words;
		return id === undefined
			? err(usageFailure('task done needs a task id'))
			: ok({ name: 'task-done', id, shouldPrintJson });
	}
	if (action !== 'add') {
		return err(usageFailure('task needs add or done'));
	}
	const section = values.section ?? 'next';
	if (!isTaskSectionName(section)) {
		return err(
			usageFailure(`--section must be one of ${TASK_SECTIONS.join(', ')}`),
		);
	}
	if (words.length === 0 || values.story === undefined) {
		return err(usageFailure('task add needs a text and --story <number>'));
	}
	return ok({
		name: 'task-add',
		text: words.join(' '),
		story: values.story,
		section,
		assignedTo: values['assigned-to'],
		isModelText,
		shouldPrintJson,
	});
}

const MAXIMUM_MINUTES_AGO = 24 * 60;

function parseTimer(
	rest: readonly string[],
	values: Flags,
	shouldPrintJson: boolean,
): Result<ParsedCommand, Failure> {
	const [action, ...words] = rest;
	switch (action) {
		case 'start':
			return parseTimerStart(words, values, shouldPrintJson);
		case 'stop':
			return ok({ name: 'timer-stop', shouldPrintJson });
		case 'continue':
			return ok({ name: 'timer-continue', shouldPrintJson });
		default:
			return err(usageFailure('timer needs start, stop or continue'));
	}
}

function parseTimerStart(
	words: readonly string[],
	values: Flags,
	shouldPrintJson: boolean,
): Result<ParsedCommand, Failure> {
	const target = words.join(' ').trim();
	if (target === '') {
		return err(usageFailure('timer start needs a task id or text'));
	}
	if (values.ago === undefined) {
		return ok({ name: 'timer-start', target, shouldPrintJson });
	}
	const minutesAgo = Number(values.ago);
	const isUsableMinutes =
		Number.isInteger(minutesAgo) &&
		minutesAgo > 0 &&
		minutesAgo <= MAXIMUM_MINUTES_AGO;
	if (!isUsableMinutes) {
		return err(usageFailure('--ago takes whole minutes from 1 to 1440'));
	}
	return ok({ name: 'timer-start', target, minutesAgo, shouldPrintJson });
}

function parseStatus(
	values: Flags,
	shouldPrintJson: boolean,
): Result<ParsedCommand, Failure> {
	const format = values.format ?? (shouldPrintJson ? 'json' : 'text');
	const statusFormat = STATUS_FORMATS.find((candidate) => candidate === format);
	if (statusFormat === undefined) {
		return err(
			usageFailure(`--format must be one of ${STATUS_FORMATS.join(', ')}`),
		);
	}
	return ok({ name: 'status', format: statusFormat });
}
