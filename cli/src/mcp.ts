#!/usr/bin/env node
import { runStdioServer } from './mcp-server.ts';

await runStdioServer({
	workingDirectory: process.cwd(),
	environment: process.env,
});
