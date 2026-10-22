import { createHash } from 'node:crypto';
import { existsSync } from 'node:fs';
import { homedir } from 'node:os';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import { err, ok, type Result } from './result.ts';

/** The two hook-core functions the migration relies on for fingerprints. */
export type ProvenanceHasher = {
	hashText: (normalisedText: string) => string;
	normalizeLines: (lines: string[], prefix?: string) => string;
};

export const DEFAULT_PROVENANCE_CORE_PATH = path.join(
	homedir(),
	'.claude',
	'hooks',
	'lib',
	'obsidian-ai-provenance-core.mjs',
);

/**
 * Loads `hashText` and `normalizeLines` from the live hook core so fingerprints
 * the migration writes are exactly the ones `check` recomputes.
 */
export async function loadProvenanceCore(
	modulePath: string = DEFAULT_PROVENANCE_CORE_PATH,
): Promise<Result<ProvenanceHasher>> {
	if (!existsSync(modulePath))
		return err(`provenance core not found at ${modulePath}`);
	const loaded: unknown = await import(pathToFileURL(modulePath).href);
	if (typeof loaded !== 'object' || loaded === null)
		return err(`provenance core at ${modulePath} did not export an object`);
	const hashText = pickFunction(loaded, 'hashText');
	const normalizeLines = pickFunction(loaded, 'normalizeLines');
	if (hashText === undefined || normalizeLines === undefined)
		return err(
			`provenance core at ${modulePath} lacks hashText/normalizeLines`,
		);
	return ok({
		hashText: (normalisedText) =>
			expectString(hashText(normalisedText), 'hashText'),
		normalizeLines: (lines, prefix = '') =>
			expectString(normalizeLines(lines, prefix), 'normalizeLines'),
	});
}

/**
 * Reference implementation of the core's hashing (sha256 of right-trimmed lines
 * with blank edges dropped, first 12 hex). Used by the parity test only.
 */
export const referenceHasher: ProvenanceHasher = {
	hashText: (normalisedText) =>
		createHash('sha256')
			.update(normalisedText, 'utf8')
			.digest('hex')
			.slice(0, 12),
	normalizeLines: (lines, prefix = '') => {
		const stripped = lines.map((line) =>
			(prefix.length > 0 && line.startsWith(prefix)
				? line.slice(prefix.length)
				: line
			).trimEnd(),
		);
		let start = 0;
		let end = stripped.length;
		while (start < end && (stripped[start] ?? '').length === 0) start++;
		while (end > start && (stripped[end - 1] ?? '').length === 0) end--;
		return stripped.slice(start, end).join('\n');
	},
};

type AnyFunction = (...parameters: unknown[]) => unknown;

function pickFunction(
	container: object,
	name: string,
): AnyFunction | undefined {
	const candidate: unknown = Reflect.get(container, name);
	return typeof candidate === 'function'
		? (...parameters: unknown[]) =>
				Reflect.apply(candidate, container, parameters)
		: undefined;
}

function expectString(value: unknown, source: string): string {
	if (typeof value !== 'string')
		throw new Error(`provenance core ${source} returned a non-string`);
	return value;
}
