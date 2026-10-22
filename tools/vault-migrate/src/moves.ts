import { existsSync } from 'node:fs';
import path from 'node:path';
import type { MoveSpec } from './profile.ts';
import { err, ok, type Result } from './result.ts';
import type { VaultEntry } from './vault-scan.ts';

export type ResolvedMove = {
	from: string;
	/** Vault-relative destination; undefined when the file goes to another vault. */
	to: string | undefined;
	/** Absolute destination for a cross-vault move. */
	externalPath?: string;
	kind: 'file' | 'directory';
};

export type ResolvedDelete = { path: string; kind: 'file' | 'directory' };

/**
 * Resolves `moves.json` against the (already renamed) vault tree. `from` may
 * use `*` for one path segment; `{0}`, `{1}`… in `to` echo the matched segments.
 * A cross-vault entry names the other vault with `toVault`, relative to this one.
 */
export function resolveMoves(
	specs: MoveSpec[],
	entries: VaultEntry[],
	vaultRoot: string,
): Result<{ moves: ResolvedMove[]; deletes: ResolvedDelete[] }> {
	const moves: ResolvedMove[] = [];
	const deletes: ResolvedDelete[] = [];
	const kindByPath = new Map(
		entries.map((entry) => [entry.relativePath, entry.kind] as const),
	);
	for (const spec of specs) {
		if ('delete' in spec) {
			const kind = kindByPath.get(spec.delete);
			if (kind === undefined) continue;
			deletes.push({ path: spec.delete, kind });
			continue;
		}
		const matches = matchPattern(spec.from, [...kindByPath.keys()]).filter(
			(match) => !(spec.except ?? []).includes(match.path),
		);
		for (const match of matches) {
			const kind = kindByPath.get(match.path);
			if (kind === undefined) continue;
			const target = match.captures.reduce(
				(text, capture, index) => text.replaceAll(`{${index}}`, capture),
				spec.to,
			);
			if (spec.toVault === undefined) {
				moves.push({ from: match.path, to: target, kind });
				continue;
			}
			const otherVault = path.resolve(vaultRoot, spec.toVault);
			if (!existsSync(path.join(otherVault, '.obsidian')))
				return err(
					`toVault ${spec.toVault} (${otherVault}) is not an Obsidian vault`,
				);
			moves.push({
				from: match.path,
				to: undefined,
				externalPath: path.join(otherVault, target),
				kind,
			});
		}
	}
	return ok({ moves, deletes });
}

type PatternMatch = { path: string; captures: string[] };

function matchPattern(pattern: string, candidates: string[]): PatternMatch[] {
	if (!pattern.includes('*'))
		return candidates.includes(pattern)
			? [{ path: pattern, captures: [] }]
			: [];
	const patternSegments = pattern.split('/');
	return candidates.flatMap((candidate) => {
		const segments = candidate.split('/');
		if (segments.length !== patternSegments.length) return [];
		const captures: string[] = [];
		for (let index = 0; index < segments.length; index++) {
			const expected = patternSegments[index];
			const actual = segments[index] ?? '';
			if (expected === '*') captures.push(actual);
			else if (expected !== actual) return [];
		}
		return [{ path: candidate, captures }];
	});
}
