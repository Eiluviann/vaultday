import { readFileSync, statSync } from 'node:fs';
import path from 'node:path';
import { normalisePath } from './paths.ts';

/**
 * Reads the `stat -f '%B%t%N'` manifest taken on the live vault before copying.
 * Paths may be absolute (the common directory prefix is stripped) or vault-relative.
 * @returns vault-relative path → local YYYY-MM-DD birth date
 */
export function readBirthtimeManifest(
	manifestPath: string,
): Map<string, string> {
	const rows = readFileSync(manifestPath, 'utf8')
		.split('\n')
		.filter((line) => line.length > 0)
		.flatMap((line) => {
			const separator = line.indexOf('\t');
			if (separator === -1) return [];
			const epochSeconds = Number(line.slice(0, separator));
			if (!Number.isFinite(epochSeconds)) return [];
			return [{ epochSeconds, filePath: line.slice(separator + 1) }];
		});
	const commonRoot = commonDirectoryPrefix(
		rows
			.filter((row) => path.isAbsolute(row.filePath))
			.map((row) => row.filePath),
	);
	const manifest = new Map<string, string>();
	for (const row of rows) {
		const relativePath = path.isAbsolute(row.filePath)
			? path.relative(commonRoot, row.filePath)
			: row.filePath;
		manifest.set(
			normalisePath(relativePath),
			formatLocalDate(new Date(row.epochSeconds * 1000)),
		);
	}
	return manifest;
}

/** Birth date of the file itself, the fallback when the manifest has no row for it. */
export function fileBirthDate(absolutePath: string): string {
	return formatLocalDate(statSync(absolutePath).birthtime);
}

export function formatLocalDate(date: Date): string {
	const year = date.getFullYear();
	const month = String(date.getMonth() + 1).padStart(2, '0');
	const day = String(date.getDate()).padStart(2, '0');
	return `${year}-${month}-${day}`;
}

function commonDirectoryPrefix(absolutePaths: string[]): string {
	const first = absolutePaths[0];
	if (first === undefined) return '';
	let prefix = path.dirname(first).split(path.sep);
	for (const filePath of absolutePaths.slice(1)) {
		const segments = path.dirname(filePath).split(path.sep);
		let shared = 0;
		while (
			shared < prefix.length &&
			shared < segments.length &&
			prefix[shared] === segments[shared]
		)
			shared++;
		prefix = prefix.slice(0, shared);
	}
	return prefix.join(path.sep);
}
