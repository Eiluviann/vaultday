import { createHash } from 'node:crypto';
import { copyFileSync, existsSync, readFileSync } from 'node:fs';
import { homedir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

type ComparisonStatus = 'identical' | 'differs' | 'missing-from-live';

type FileComparison = {
	fileName: string;
	status: ComparisonStatus;
};

const VENDORED_FILE_NAMES = [
	'obsidian-ai-provenance-core.mjs',
	'obsidian-ai-provenance-core.test.mjs',
	'obsidian-ai-provenance-verification.test.mjs',
] as const;

const LIVE_HOOK_LIBRARY_DIRECTORY = path.join(
	homedir(),
	'.claude',
	'hooks',
	'lib',
);
const VENDOR_DIRECTORY = path.resolve(
	path.dirname(fileURLToPath(import.meta.url)),
	'..',
	'vendor',
);

/**
 * Compares the vendored provenance engine with the live hook copy on this machine.
 *
 * With `--sync` the live files are copied over the vendored ones instead.
 * Exits 0 when the copies match or when no live copy exists (CI), 1 when they
 * differ, when the live hook lacks one of the files, or when a sync cannot run.
 */
function main(argv: readonly string[]): number {
	const shouldSync = argv.includes('--sync');
	if (!existsSync(LIVE_HOOK_LIBRARY_DIRECTORY)) {
		console.log(
			`parity: no live hook at ${LIVE_HOOK_LIBRARY_DIRECTORY}, skipping`,
		);
		return 0;
	}
	if (shouldSync) {
		const missingFileNames = syncVendoredFilesFromLive();
		if (missingFileNames.length > 0) {
			console.error(
				`parity: cannot sync, the live hook lacks ${missingFileNames.join(', ')}`,
			);
			return 1;
		}
		console.log('parity: vendored files refreshed from the live hook');
		return 0;
	}
	const comparisons = VENDORED_FILE_NAMES.map(compareVendoredFileWithLive);
	const driftedComparisons = comparisons.filter(
		(comparison) => comparison.status !== 'identical',
	);
	if (driftedComparisons.length === 0) {
		console.log('parity: vendored provenance core matches the live hook');
		return 0;
	}
	const driftDescription = driftedComparisons.map(describeDrift).join(', ');
	console.error(
		`parity: vendored files drift from the live hook: ${driftDescription}. Run "pnpm sync" to refresh them.`,
	);
	return 1;
}

function compareVendoredFileWithLive(fileName: string): FileComparison {
	const liveFilePath = path.join(LIVE_HOOK_LIBRARY_DIRECTORY, fileName);
	// A file the hook no longer ships is drift to report, not a broken checkout.
	if (!existsSync(liveFilePath)) {
		return { fileName, status: 'missing-from-live' };
	}
	const vendoredHash = hashFile(path.join(VENDOR_DIRECTORY, fileName));
	const liveHash = hashFile(liveFilePath);
	return {
		fileName,
		status: vendoredHash === liveHash ? 'identical' : 'differs',
	};
}

/** Copies the live files over the vendored ones; returns the live files that are missing, in which case nothing is copied. */
function syncVendoredFilesFromLive(): string[] {
	const missingFileNames = VENDORED_FILE_NAMES.filter(
		(fileName) => !existsSync(path.join(LIVE_HOOK_LIBRARY_DIRECTORY, fileName)),
	);
	if (missingFileNames.length > 0) return missingFileNames;
	for (const fileName of VENDORED_FILE_NAMES) {
		copyFileSync(
			path.join(LIVE_HOOK_LIBRARY_DIRECTORY, fileName),
			path.join(VENDOR_DIRECTORY, fileName),
		);
	}
	return [];
}

function describeDrift(comparison: FileComparison): string {
	return comparison.status === 'missing-from-live'
		? `${comparison.fileName} (missing from the live hook)`
		: comparison.fileName;
}

/** Hashes a vendored file; a missing one means the checkout itself is broken. */
function hashFile(filePath: string): string {
	if (!existsSync(filePath))
		throw new Error(`parity: missing file ${filePath}`);
	return createHash('sha256').update(readFileSync(filePath)).digest('hex');
}

process.exitCode = main(process.argv.slice(2));
