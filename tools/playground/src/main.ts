import { execFileSync } from 'node:child_process';
import { homedir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { planCalendar } from './calendar.ts';
import { LAUNCHER_NAME } from './launcher.ts';
import { MOCK_STUDIO_FOLDER } from './mock-studio.ts';
import { parseOptions } from './options.ts';
import { PERSONAL_FOLDER } from './personal.ts';
import { buildPlaygroundPlan } from './playground.ts';
import { writePlayground } from './write-playground.ts';

const USAGE = `Usage: scripts/playground.sh [--out <folder>] [--today YYYY-MM-DD] [--now HH:MM]

Writes the playground's two workspace roots, "${MOCK_STUDIO_FOLDER}" (work) and "${PERSONAL_FOLDER}" (life),
and "${LAUNCHER_NAME}" into --out (default ~/Vaultday-playground). Run it again to reset the notes.`;

const REPOSITORY_ROOT = path.resolve(
	path.dirname(fileURLToPath(import.meta.url)),
	'../../..',
);

process.exitCode = main(process.argv.slice(2));

function main(argv: string[]): number {
	const optionsResult = parseOptions(argv, {
		homeDirectory: homedir(),
		repositoryRoots: repositoryRoots(),
		clock: new Date(),
	});
	if (!optionsResult.ok) {
		console.error(`${optionsResult.error}\n\n${USAGE}`);
		return 2;
	}
	const options = optionsResult.value;
	const calendar = planCalendar(options.today, options.now);
	const plan = buildPlaygroundPlan({
		outDirectory: options.outDirectory,
		calendar,
		fixturesRoot: path.join(REPOSITORY_ROOT, 'core/fixtures'),
	});
	const written = writePlayground(plan);
	if (!written.ok) {
		console.error(written.error);
		return 1;
	}
	const report = written.value;
	console.log(
		`Wrote ${report.writtenFiles} files to ${options.outDirectory} (week of ${calendar.focusMonday}, today ${calendar.today} ${calendar.now}).`,
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

/** This checkout and, for a worktree, the primary checkout that holds it. */
function repositoryRoots(): string[] {
	try {
		const commonGitDirectory = execFileSync(
			'git',
			[
				'-C',
				REPOSITORY_ROOT,
				'rev-parse',
				'--path-format=absolute',
				'--git-common-dir',
			],
			{ encoding: 'utf8' },
		).trim();
		return [REPOSITORY_ROOT, path.dirname(commonGitDirectory)];
	} catch {
		return [REPOSITORY_ROOT];
	}
}

function isVaultdayRunning(): boolean {
	try {
		execFileSync('pgrep', ['-x', 'Vaultday'], { stdio: 'ignore' });
		return true;
	} catch {
		return false;
	}
}
