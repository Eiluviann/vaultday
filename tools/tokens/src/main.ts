import { mkdirSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { parseArgs } from 'node:util';
import {
	generateTokensSwiftFromFile,
	readTextFile,
} from './generate-from-file.ts';
import {
	DEFAULT_SWIFT_OUTPUT_PATH,
	DEFAULT_TOKENS_PATH,
	REPOSITORY_ROOT,
} from './paths.ts';
import { err, ok, type Result } from './result.ts';

type CliOptions = {
	isCheck: boolean;
	tokensPath: string;
	outputPath: string;
};

const USAGE = `usage: tsx src/main.ts [--check] [--input <tokens.json>] [--output <Tokens.swift>]
Paths are relative to the repository root.`;

process.exitCode = main(process.argv.slice(2));

function main(argv: string[]): number {
	const options = parseCliOptions(argv);
	if (!options.ok) {
		console.error(`${options.error}\n\n${USAGE}`);
		return 2;
	}
	const generated = generateTokensSwiftFromFile(options.value.tokensPath);
	if (!generated.ok) {
		console.error(generated.error);
		return 1;
	}
	return options.value.isCheck
		? checkOutput(options.value.outputPath, generated.value)
		: writeOutput(options.value.outputPath, generated.value);
}

function parseCliOptions(argv: string[]): Result<CliOptions> {
	try {
		const { values } = parseArgs({
			args: argv,
			options: {
				check: { type: 'boolean' },
				input: { type: 'string' },
				output: { type: 'string' },
			},
			allowPositionals: false,
		});
		return ok({
			isCheck: values.check ?? false,
			tokensPath: path.resolve(
				REPOSITORY_ROOT,
				values.input ?? DEFAULT_TOKENS_PATH,
			),
			outputPath: path.resolve(
				REPOSITORY_ROOT,
				values.output ?? DEFAULT_SWIFT_OUTPUT_PATH,
			),
		});
	} catch (error) {
		return err(error instanceof Error ? error.message : String(error));
	}
}

function checkOutput(outputPath: string, generated: string): number {
	const committed = readTextFile(outputPath);
	const relativePath = path.relative(REPOSITORY_ROOT, outputPath);
	if (committed.ok && committed.value === generated) {
		console.log(`${relativePath} is up to date.`);
		return 0;
	}
	console.error(
		`${relativePath} is ${committed.ok ? 'out of date' : 'missing'}; run \`pnpm --filter @vaultday/tokens generate\` and commit the result.`,
	);
	return 1;
}

function writeOutput(outputPath: string, generated: string): number {
	mkdirSync(path.dirname(outputPath), { recursive: true });
	writeFileSync(outputPath, generated);
	console.log(`wrote ${path.relative(REPOSITORY_ROOT, outputPath)}`);
	return 0;
}
