import {
	type ApiCommand,
	type ParsedCommand,
	parseArguments,
	USAGE,
} from './arguments.ts';
import {
	type CommandResult,
	runLog,
	runReport,
	runTaskAdd,
	runTaskDone,
	runToday,
} from './commands.ts';
import { resolveWorkspaceFrom } from './core.ts';
import type { Environment } from './provenance.ts';
import { modelTextPolicy } from './provenance.ts';
import {
	runStatus,
	runTimerContinue,
	runTimerStart,
	runTimerStop,
} from './timer-commands.ts';

export type CliContext = {
	readonly workingDirectory: string;
	readonly environment: Environment;
	readonly writeOut: (text: string) => void;
	readonly writeError: (text: string) => void;
};

const EXIT_FAILURE = 1;
const EXIT_USAGE = 2;

/** Runs one command line and returns the process exit code. */
export async function runCli(
	argv: readonly string[],
	context: CliContext,
): Promise<number> {
	const parsed = parseArguments(argv);
	if (!parsed.ok) {
		context.writeError(`vaultday: ${parsed.error.message}\n${USAGE}\n`);
		return EXIT_USAGE;
	}
	const command = parsed.value;
	if (command.name === 'help') {
		if (command.isRequested) {
			context.writeOut(`${USAGE}\n`);
			return 0;
		}
		context.writeError(`${USAGE}\n`);
		return EXIT_USAGE;
	}
	if (command.name === 'mcp') {
		const { runStdioServer } = await import('./mcp-server.ts');
		await runStdioServer(context);
		return 0;
	}
	if (isApiCommand(command)) {
		return printApiResult(context, await executeApi(command, context), command);
	}
	const startDirectory =
		command.name === 'today' && command.startDirectory !== undefined
			? command.startDirectory
			: context.workingDirectory;
	const workspace = resolveWorkspaceFrom(startDirectory);
	if (!workspace.ok) return fail(context, workspace.error.message);
	const outcome = execute(command, workspace.value, context);
	if (!outcome.result.ok) return fail(context, outcome.result.error.message);
	const { value, text } = outcome.result.value;
	context.writeOut(
		`${outcome.shouldPrintJson ? JSON.stringify(value, undefined, 2) : text}\n`,
	);
	return 0;
}

function isApiCommand(command: ParsedCommand): command is ApiCommand {
	return (
		command.name === 'timer-start' ||
		command.name === 'timer-stop' ||
		command.name === 'timer-continue' ||
		command.name === 'status'
	);
}

/** The timer and status commands: API only, so they need no workspace. */
function executeApi(
	command: ApiCommand,
	context: CliContext,
): Promise<CommandResult> {
	switch (command.name) {
		case 'timer-start':
			return runTimerStart(context.environment, {
				target: command.target,
				minutesAgo: command.minutesAgo,
			});
		case 'timer-stop':
			return runTimerStop(context.environment);
		case 'timer-continue':
			return runTimerContinue(context.environment);
		case 'status':
			return runStatus(context.environment, command.format);
	}
}

function printApiResult(
	context: CliContext,
	result: CommandResult,
	command: ApiCommand,
): number {
	if (!result.ok) return fail(context, result.error.message);
	const shouldPrintJson =
		command.name === 'status'
			? command.format === 'json'
			: command.shouldPrintJson;
	const { value, text } = result.value;
	context.writeOut(
		`${shouldPrintJson ? JSON.stringify(value, undefined, 2) : text}\n`,
	);
	return 0;
}

function execute(
	command: Exclude<ParsedCommand, { name: 'help' | 'mcp' } | ApiCommand>,
	workspace: Parameters<typeof runToday>[0],
	context: CliContext,
): { result: CommandResult; shouldPrintJson: boolean } {
	switch (command.name) {
		case 'today':
			return {
				result: runToday(workspace, command.date),
				shouldPrintJson: command.shouldPrintJson,
			};
		case 'log':
			return {
				result: runLog(workspace, {
					kind: command.kind,
					text: command.text,
					story: command.story,
					task: command.task,
					date: command.date,
					time: command.time,
					textPolicy: modelTextPolicy(command.isModelText, context.environment),
				}),
				shouldPrintJson: command.shouldPrintJson,
			};
		case 'report':
			// A report is a JSON bundle; its text form is the same JSON.
			return {
				result: runReport(workspace, command),
				shouldPrintJson: false,
			};
		case 'task-add':
			return {
				result: runTaskAdd(workspace, {
					text: command.text,
					story: command.story,
					section: command.section,
					assignedTo: command.assignedTo,
					textPolicy: modelTextPolicy(command.isModelText, context.environment),
				}),
				shouldPrintJson: command.shouldPrintJson,
			};
		case 'task-done':
			return {
				result: runTaskDone(workspace, command.id),
				shouldPrintJson: command.shouldPrintJson,
			};
	}
}

function fail(context: CliContext, message: string): number {
	context.writeError(`vaultday: ${message}\n`);
	return EXIT_FAILURE;
}
