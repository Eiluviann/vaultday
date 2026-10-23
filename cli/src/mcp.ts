#!/usr/bin/env node
import { logicalWorkingDirectory } from './core.ts';
import { runStdioServer } from './mcp-server.ts';

await runStdioServer({
	workingDirectory: logicalWorkingDirectory(process.cwd(), process.env),
	environment: process.env,
});
