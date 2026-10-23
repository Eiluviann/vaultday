import { writeFileSync } from 'node:fs';
import path from 'node:path';
import { parseArgs } from 'node:util';
import { generateKeymapDoc, readText } from './generate.ts';
import { KEYMAP_DOC_PATH, KEYMAP_PATH, REPOSITORY_ROOT } from './paths.ts';

const USAGE = 'usage: tsx src/main.ts [--check]';

process.exitCode = main(process.argv.slice(2));

function main(argv: string[]): number {
	const isCheck = parseIsCheck(argv);
	if (isCheck === undefined) {
		console.error(USAGE);
		return 2;
	}
	const generated = generateKeymapDoc(
		path.resolve(REPOSITORY_ROOT, KEYMAP_PATH),
	);
	if (!generated.ok) {
		console.error(generated.error);
		return 1;
	}
	const outputPath = path.resolve(REPOSITORY_ROOT, KEYMAP_DOC_PATH);
	return isCheck
		? checkOutput(outputPath, generated.value)
		: writeOutput(outputPath, generated.value);
}

function parseIsCheck(argv: string[]): boolean | undefined {
	try {
		const { values } = parseArgs({
			args: argv,
			options: { check: { type: 'boolean' } },
			allowPositionals: false,
		});
		return values.check ?? false;
	} catch {
		return undefined;
	}
}

function checkOutput(outputPath: string, generated: string): number {
	const committed = readText(outputPath);
	if (committed.ok && committed.value === generated) {
		console.log(`${KEYMAP_DOC_PATH} is up to date.`);
		return 0;
	}
	console.error(
		`${KEYMAP_DOC_PATH} is ${committed.ok ? 'out of date' : 'missing'}; run \`pnpm --filter @vaultday/keymap-doc generate\` and commit the result.`,
	);
	return 1;
}

function writeOutput(outputPath: string, generated: string): number {
	writeFileSync(outputPath, generated);
	console.log(`wrote ${KEYMAP_DOC_PATH}`);
	return 0;
}
