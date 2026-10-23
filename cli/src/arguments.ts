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
	  };

export const USAGE = `Usage:
  vaultday today [path] [--date YYYY-MM-DD] [--json]
  vaultday log <${LOG_KINDS.join('|')}> <text> [--story <number>] [--task <id>] [--date YYYY-MM-DD] [--time HH:MM] [--ai|--human] [--json]
  vaultday report <story number|client> --from YYYY-MM-DD --to YYYY-MM-DD
  vaultday task add <text> --story <number> [--section next|pending] [--assigned-to <who>] [--ai|--human] [--json]
  vaultday task done <id> [--json]
  vaultday mcp

The workspace is the deepest .vaultday/ at or above the working directory.
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
