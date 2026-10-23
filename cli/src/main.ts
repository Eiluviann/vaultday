#!/usr/bin/env node
import { runCli } from './cli.ts';
import { logicalWorkingDirectory } from './core.ts';

process.exitCode = await runCli(process.argv.slice(2), {
	workingDirectory: logicalWorkingDirectory(process.cwd(), process.env),
	environment: process.env,
	writeOut: (text) => process.stdout.write(text),
	writeError: (text) => process.stderr.write(text),
});
