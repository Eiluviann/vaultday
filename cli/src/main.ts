#!/usr/bin/env node
import { parseArguments, USAGE } from './arguments.ts';
import { loadToday, renderToday } from './today.ts';

function main(argv: readonly string[]): number {
	const parsed = parseArguments(argv);
	if (parsed.command === 'help') {
		console.error(USAGE);
		return 2;
	}
	const result = loadToday({
		workspacePath: parsed.workspacePath,
		date: parsed.date,
	});
	if (!result.ok) {
		console.error(`vaultday: ${result.error.message}`);
		return 1;
	}
	console.log(renderToday(result.value.view));
	if (parsed.shouldPrintTiming) {
		console.error(
			`today() took ${result.value.callMilliseconds.toFixed(2)} ms`,
		);
	}
	return 0;
}

process.exitCode = main(process.argv.slice(2));
