import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js';
import { z } from 'zod';
import type { CliContext } from './cli.ts';
import {
	type CommandResult,
	LOG_KINDS,
	runLog,
	runReport,
	runTaskAdd,
	runTaskDone,
	runToday,
	TASK_SECTIONS,
} from './commands.ts';
import { resolveWorkspaceFrom } from './core.ts';
import { modelTextPolicy } from './provenance.ts';

type ToolContext = Pick<CliContext, 'workingDirectory' | 'environment'>;

const DATE = z.string().regex(/^\d{4}-\d{2}-\d{2}$/, 'YYYY-MM-DD');
const IS_MODEL_TEXT = z
	.boolean()
	.optional()
	.describe(
		'True (the default for this server) when the text was written by a model: the line is attributed to the model. Pass false for a fact the user dictated.',
	);

/**
 * Builds the stdio MCP server: the CLI's commands as tools. The workspace is
 * resolved from the server's working directory on every call, so a session
 * opened in a story folder works on that folder's workspace.
 */
export function createVaultdayServer(context: ToolContext): McpServer {
	const server = new McpServer({ name: 'vaultday', version: '0.1.0' });

	server.registerTool(
		'today',
		{
			title: 'Today',
			description:
				"Today's stories with their open Next Steps and Pending Input tasks, the day's log lines and the items waiting on someone else.",
			inputSchema: {
				date: DATE.optional().describe('Day to show; defaults to today.'),
			},
		},
		({ date }) => answer(context, (workspace) => runToday(workspace, date)),
	);

	server.registerTool(
		'log',
		{
			title: 'Log',
			description:
				'Append a line to the day log (Log/YYYY-MM-DD.md), optionally tied to a story and a task.',
			inputSchema: {
				kind: z.enum(LOG_KINDS),
				text: z
					.string()
					.min(1)
					.describe('One line, no inline [key:: value] fields or pipes.'),
				story: z.string().optional().describe('Story number, such as WEB-142.'),
				task: z.string().optional().describe('Task id, such as task-5.'),
				isModelText: IS_MODEL_TEXT,
			},
		},
		({ kind, text, story, task, isModelText }) =>
			answer(context, (workspace) =>
				runLog(workspace, {
					kind,
					text,
					story,
					task,
					textPolicy: modelTextPolicy(isModelText ?? true, context.environment),
				}),
			),
	);

	server.registerTool(
		'report',
		{
			title: 'Report',
			description:
				'JSON bundle for a story number or a client over a date range: frontmatter, Next Steps and Pending Input state, log lines and an authorship summary.',
			inputSchema: {
				subject: z
					.string()
					.min(1)
					.describe(
						'A story number or a client (project key, title or folder).',
					),
				from: DATE,
				to: DATE,
			},
		},
		({ subject, from, to }) =>
			answer(context, (workspace) =>
				runReport(workspace, { subject, from, to }),
			),
	);

	server.registerTool(
		'task_add',
		{
			title: 'Add task',
			description: 'Add a task under Next Steps or Pending Input of a story.',
			inputSchema: {
				text: z
					.string()
					.min(1)
					.describe('One line, no inline [key:: value] fields.'),
				story: z.string().min(1).describe('Story number, such as WEB-142.'),
				section: z
					.enum(TASK_SECTIONS)
					.optional()
					.describe('next (default) or pending.'),
				assignedTo: z
					.string()
					.optional()
					.describe('Who a pending task waits on.'),
				isModelText: IS_MODEL_TEXT,
			},
		},
		({ text, story, section, assignedTo, isModelText }) =>
			answer(context, (workspace) =>
				runTaskAdd(workspace, {
					text,
					story,
					section: section ?? 'next',
					assignedTo,
					textPolicy: modelTextPolicy(isModelText ?? true, context.environment),
				}),
			),
	);

	server.registerTool(
		'task_done',
		{
			title: 'Tick task done',
			description: 'Mark a task done by its id; only that line changes.',
			inputSchema: {
				id: z.string().min(1).describe('Task id, such as task-5.'),
			},
		},
		({ id }) => answer(context, (workspace) => runTaskDone(workspace, id)),
	);

	return server;
}

/** Serves the tools over stdio until the client disconnects. */
export async function runStdioServer(context: ToolContext): Promise<void> {
	await createVaultdayServer(context).connect(new StdioServerTransport());
}

function answer(
	context: ToolContext,
	run: (workspace: Parameters<typeof runToday>[0]) => CommandResult,
) {
	const workspace = resolveWorkspaceFrom(context.workingDirectory);
	const result = workspace.ok ? run(workspace.value) : workspace;
	if (!result.ok) {
		return {
			isError: true,
			content: [{ type: 'text' as const, text: result.error.message }],
		};
	}
	return { content: [{ type: 'text' as const, text: result.value.text }] };
}
