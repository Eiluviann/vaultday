import { classifyLines } from './markdown-regions.ts';

/**
 * Lenient view of `%% ai:… %%` units: the tool never interprets marker fields
 * beyond the hash it may refresh, so any marker line the hook core might accept
 * (bare, author-only, stamped, verified) is recognised by its head alone.
 */
export type ProvenanceUnit = {
	kind: 'note' | 'block';
	/** 0-based index of the `ai:note` / `ai:begin` line. */
	markerLine: number;
	/** 0-based inclusive range of the hashed body lines. */
	bodyStart: number;
	bodyEnd: number;
	prefix: string;
	/** The 12-hex fingerprint currently on the marker; undefined for bare markers. */
	hash: string | undefined;
};

const BEGIN_PATTERN =
	/^(?<prefix>(?:> )*)%% ai:(?<kind>note|begin)(?: [^\s·]+(?: · \d{4}-\d{2}-\d{2} · (?<hash>[0-9a-f]{12}))?)?(?: · verified \d{4}-\d{2}-\d{2} · [0-9a-f]{12})? %%$/;
const END_PATTERN = /^(?:> )*%% ai:end %%$/;
/** Captures everything up to the hash, the hash, and everything after it (verify token and closer). */
export const STAMPED_MARKER_HASH_PATTERN =
	/^((?:> )*%% ai:(?:note|begin) [^\s·]+ · \d{4}-\d{2}-\d{2} · )([0-9a-f]{12})(.*)$/;

export function scanUnits(lines: string[]): ProvenanceUnit[] {
	const regions = classifyLines(lines);
	const units: ProvenanceUnit[] = [];
	let openBlock:
		| { markerLine: number; prefix: string; hash: string | undefined }
		| undefined;
	for (let index = 0; index < lines.length; index++) {
		if (regions[index]?.region !== 'marker') continue;
		const line = (lines[index] ?? '').replace(/\r$/, '');
		const beginMatch = BEGIN_PATTERN.exec(line);
		if (beginMatch?.groups !== undefined) {
			const prefix = beginMatch.groups.prefix ?? '';
			const hash = beginMatch.groups.hash;
			if (beginMatch.groups.kind === 'note') {
				if (openBlock === undefined)
					units.push({
						kind: 'note',
						markerLine: index,
						bodyStart: index + 1,
						bodyEnd: lines.length - 1,
						prefix,
						hash,
					});
			} else if (openBlock === undefined) {
				openBlock = { markerLine: index, prefix, hash };
			}
			continue;
		}
		if (END_PATTERN.test(line) && openBlock !== undefined) {
			units.push({
				kind: 'block',
				markerLine: openBlock.markerLine,
				bodyStart: openBlock.markerLine + 1,
				bodyEnd: index - 1,
				prefix: openBlock.prefix,
				hash: openBlock.hash,
			});
			openBlock = undefined;
		}
	}
	return units;
}

/** True when the 0-based line index lies inside a unit's hashed body. */
export function isInsideUnit(
	units: ProvenanceUnit[],
	lineIndex: number,
): boolean {
	return units.some(
		(unit) => unit.bodyStart <= lineIndex && lineIndex <= unit.bodyEnd,
	);
}
