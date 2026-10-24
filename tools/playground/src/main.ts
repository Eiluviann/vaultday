import { execFileSync } from 'node:child_process';
import { homedir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';
import {
	isValidClockTime,
	isValidIsoDate,
	localDateAndTime,
	planCalendar,
} from './calendar.ts';
import { LAUNCHER_NAME } from './launcher.ts';
import { MOCK_STUDIO_FOLDER } from './mock-studio.ts';
import { PERSONAL_FOLDER } from './personal.ts';
import { buildPlaygroundPlan } from './playground.ts';
import { type Result, writePlayground } from './write-playground.ts';

type PlaygroundOptions = {
	outDirectory: string;
	archiveDirectory: string;
	today: string;
	now: string;
};

const USAGE = `Usage: pnpm --filter @vaultday/playground start -- [--out <folder>] [--today YYYY-MM-DD] [--now HH:MM]

Writes the playground's two workspace roots, "${MOCK_STUDIO_FOLDER}" (work) and "${PERSONAL_FOLDER}" (life),
and "${LAUNCHER_NAME}" into --out (default ~/Vaultday-playground). Run it again to reset the notes.`;

const FIXTURES_ROOT = path.resolve(
	path.dirname(fileURLToPath(import.meta.url)),
	'../../../core/fixtures',
);
/** Folders the generator never writes into: the user's vaults and any app container. */
const PROTECTED_FOLDERS = ['Vaults', 'Library'].map((name) =>
	path.join(homedir(), name),
);

// `pnpm start -- --out …` hands the separator on.
process.exitCode = main(
	process.argv
		.slice(2)
		.filter((argument, index) => index !== 0 || argument !== '--'),
);

function main(argv: string[]): number {
	const optionsResult = parseOptions(argv);
	if (!optionsResult.ok) {
		console.error(`${optionsResult.error}\n\n${USAGE}`);
		return 2;
	}
	const options = optionsResult.value;
	const calendar = planCalendar(options.today, options.now);
	const plan = buildPlaygroundPlan({
		outDirectory: options.outDirectory,
		archiveDirectory: options.archiveDirectory,
		calendar,
		fixturesRoot: FIXTURES_ROOT,
	});
	const written = writePlayground(
		plan,
		`${options.today}T${options.now.replace(':', '')}`,
	);
	if (!written.ok) {
		console.error(written.error);
		return 1;
	}
	const report = written.value;
	console.log(
		`Wrote ${report.writtenFiles} files to ${options.outDirectory} (week of ${calendar.focusMonday}, today ${calendar.today} ${calendar.now}).`,
	);
	if (report.archivedTo !== undefined)
		console.log(
			`Moved the earlier single-root playground to ${report.archivedTo}.`,
		);
	if (report.keptAppState.length > 0)
		console.log(
			`Kept Vaultday's own state (.vaultday/sync) in ${report.keptAppState.join(' and ')}.`,
		);
	if (isVaultdayRunning())
		console.log(
			'Vaultday is running: it picks the new notes up from its folder watch.',
		);
	console.log(
		`Next: double-click "${path.join(options.outDirectory, LAUNCHER_NAME)}".`,
	);
	return 0;
}

function parseOptions(argv: string[]): Result<PlaygroundOptions> {
	let parsed: ReturnType<typeof parseArgsStrict>;
	try {
		parsed = parseArgsStrict(argv);
	} catch (error) {
		return {
			ok: false,
			error: error instanceof Error ? error.message : String(error),
		};
	}
	const defaults = localDateAndTime(new Date());
	const today = parsed.values.today ?? defaults.date;
	const now = parsed.values.now ?? defaults.time;
	if (!isValidIsoDate(today))
		return { ok: false, error: `--today is not a date: ${today}` };
	if (!isValidClockTime(now))
		return { ok: false, error: `--now is not a time: ${now}` };
	const outDirectory = path.resolve(
		expandHome(parsed.values.out ?? '~/Vaultday-playground'),
	);
	const protectedFolder = PROTECTED_FOLDERS.find(
		(folder) =>
			outDirectory === folder ||
			outDirectory.startsWith(`${folder}${path.sep}`),
	);
	if (protectedFolder !== undefined)
		return {
			ok: false,
			error: `--out lies in ${protectedFolder}; the playground never goes there`,
		};
	return {
		ok: true,
		value: {
			outDirectory,
			archiveDirectory: `${outDirectory}-archive`,
			today,
			now,
		},
	};
}

function parseArgsStrict(argv: string[]) {
	return parseArgs({
		args: argv,
		options: {
			out: { type: 'string' },
			today: { type: 'string' },
			now: { type: 'string' },
		},
		strict: true,
		allowPositionals: false,
	});
}

function expandHome(folder: string): string {
	return folder === '~' || folder.startsWith('~/')
		? path.join(homedir(), folder.slice(1))
		: folder;
}

function isVaultdayRunning(): boolean {
	try {
		execFileSync('pgrep', ['-x', 'Vaultday'], { stdio: 'ignore' });
		return true;
	} catch {
		return false;
	}
}
