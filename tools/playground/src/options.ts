import { existsSync, realpathSync } from 'node:fs';
import path from 'node:path';
import { parseArgs } from 'node:util';
import {
	type ClockTime,
	type IsoDate,
	isValidClockTime,
	isValidIsoDate,
	localDateAndTime,
} from './calendar.ts';
import type { Result } from './write-playground.ts';

/** A run's validated options. */
export type PlaygroundOptions = {
	outDirectory: string;
	today: IsoDate;
	now: ClockTime;
};

/** What the options depend on outside the arguments, injected so tests pin them. */
export type OptionsEnvironment = {
	homeDirectory: string;
	/** The vaultday checkouts (this worktree and the primary one); the playground never goes inside them, fixtures included. */
	repositoryRoots: string[];
	clock: Date;
};

const DEFAULT_OUT = '~/Vaultday-playground';

/**
 * Parses the generator's arguments. `--out` is refused when its real path
 * (symlinks resolved, compared without case, as this Mac's disk does) lies in
 * the user's vaults, `~/Library` or a vaultday checkout.
 *
 * @returns The options, or the usage error to print.
 */
export function parseOptions(
	argv: string[],
	environment: OptionsEnvironment,
): Result<PlaygroundOptions> {
	const parsed = parseArguments(argv);
	if (!parsed.ok) return parsed;
	const defaults = localDateAndTime(environment.clock);
	const today = parsed.value.today ?? defaults.date;
	const now = parsed.value.now ?? defaults.time;
	if (!isValidIsoDate(today))
		return { ok: false, error: `--today is not a date: ${today}` };
	if (!isValidClockTime(now))
		return { ok: false, error: `--now is not a time: ${now}` };
	const outDirectory = path.resolve(
		expandHome(parsed.value.out ?? DEFAULT_OUT, environment.homeDirectory),
	);
	const protectedFolder = protectedFolderHolding(outDirectory, environment);
	if (protectedFolder !== undefined)
		return {
			ok: false,
			error: `--out lies in ${protectedFolder}; the playground never goes there`,
		};
	return { ok: true, value: { outDirectory, today, now } };
}

function parseArguments(
	argv: string[],
): Result<{ out?: string; today?: string; now?: string }> {
	// `pnpm start -- --out …` hands the separator on.
	const args = argv[0] === '--' ? argv.slice(1) : argv;
	try {
		const { values } = parseArgs({
			args,
			options: {
				out: { type: 'string' },
				today: { type: 'string' },
				now: { type: 'string' },
			},
			strict: true,
			allowPositionals: false,
		});
		return {
			ok: true,
			value: {
				...(values.out === undefined ? {} : { out: values.out }),
				...(values.today === undefined ? {} : { today: values.today }),
				...(values.now === undefined ? {} : { now: values.now }),
			},
		};
	} catch (error) {
		return {
			ok: false,
			error: error instanceof Error ? error.message : String(error),
		};
	}
}

function protectedFolderHolding(
	outDirectory: string,
	environment: OptionsEnvironment,
): string | undefined {
	const out = comparablePath(outDirectory);
	return [
		path.join(environment.homeDirectory, 'Vaults'),
		path.join(environment.homeDirectory, 'Library'),
		...environment.repositoryRoots,
	].find((folder) => {
		const guarded = comparablePath(folder);
		return out === guarded || out.startsWith(`${guarded}/`);
	});
}

/**
 * The real path, lowercased: the nearest existing ancestor resolves its
 * symlinks, and the folders still to be made are appended as written.
 */
function comparablePath(folder: string): string {
	const missing: string[] = [];
	let existing = path.resolve(folder);
	while (!existsSync(existing)) {
		const parent = path.dirname(existing);
		if (parent === existing) break;
		missing.unshift(path.basename(existing));
		existing = parent;
	}
	return path.join(realpathSync(existing), ...missing).toLowerCase();
}

function expandHome(folder: string, homeDirectory: string): string {
	return folder === '~' || folder.startsWith('~/')
		? path.join(homeDirectory, folder.slice(1))
		: folder;
}
