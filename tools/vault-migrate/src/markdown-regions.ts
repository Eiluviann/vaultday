export type LineRegion = 'frontmatter' | 'fence' | 'marker' | 'prose';

export type RegionLine = {
	region: LineRegion;
	/** Language of the enclosing fenced block; '' for an untagged fence. Only set for 'fence'. */
	fenceLanguage?: string;
};

const FENCE_PATTERN = /^(?:> )*\s*(?:```|~~~)(\S*)/;
const MARKER_LINE_PATTERN = /^(?:> )*%% ai:/;
const FRONTMATTER_DELIMITERS = new Set(['---', '...']);

/**
 * Classifies every line so transforms can stay out of frontmatter, fenced code
 * and `%% ai:` marker lines. Mirrors the provenance hook: any fence line toggles
 * the fenced state, so a marker quoted inside a code block is never a marker.
 */
export function classifyLines(lines: string[]): RegionLine[] {
	const regions: RegionLine[] = [];
	const bodyStart = findBodyStart(lines);
	for (let index = 0; index < bodyStart; index++)
		regions.push({ region: 'frontmatter' });
	let fenceLanguage: string | undefined;
	for (let index = bodyStart; index < lines.length; index++) {
		const line = lines[index] ?? '';
		const fenceMatch = FENCE_PATTERN.exec(line);
		if (fenceMatch !== null) {
			if (fenceLanguage === undefined) {
				fenceLanguage = fenceMatch[1] ?? '';
				regions.push({ region: 'fence', fenceLanguage });
			} else {
				regions.push({ region: 'fence', fenceLanguage });
				fenceLanguage = undefined;
			}
			continue;
		}
		if (fenceLanguage !== undefined) {
			regions.push({ region: 'fence', fenceLanguage });
			continue;
		}
		regions.push({
			region: MARKER_LINE_PATTERN.test(line) ? 'marker' : 'prose',
		});
	}
	return regions;
}

/** Index of the first line after the frontmatter block, 0 when there is none. */
export function findBodyStart(lines: string[]): number {
	if (lines[0] !== '---') return 0;
	for (let index = 1; index < lines.length; index++) {
		if (FRONTMATTER_DELIMITERS.has(lines[index] ?? '')) return index + 1;
	}
	return 0;
}

/** True when the line opens or closes a fenced code block. */
export function isFenceLine(line: string): boolean {
	return FENCE_PATTERN.test(line);
}

/**
 * Applies `transform` to the parts of a line outside inline code spans and
 * leaves the backtick spans untouched.
 */
export function mapOutsideInlineCode(
	line: string,
	transform: (segment: string) => string,
): string {
	return splitInlineCode(line)
		.map((segment) => (segment.isCode ? segment.text : transform(segment.text)))
		.join('');
}

/** Applies `transform` to the text inside inline code spans (backticks kept). */
export function mapInsideInlineCode(
	line: string,
	transform: (code: string) => string,
): string {
	return splitInlineCode(line)
		.map((segment) =>
			segment.isCode
				? `\`${transform(segment.text.slice(1, -1))}\``
				: segment.text,
		)
		.join('');
}

type InlineSegment = { text: string; isCode: boolean };

function splitInlineCode(line: string): InlineSegment[] {
	const segments: InlineSegment[] = [];
	const pattern = /`[^`]*`/g;
	let lastIndex = 0;
	for (const match of line.matchAll(pattern)) {
		const start = match.index;
		if (start > lastIndex)
			segments.push({ text: line.slice(lastIndex, start), isCode: false });
		segments.push({ text: match[0], isCode: true });
		lastIndex = start + match[0].length;
	}
	if (lastIndex < line.length)
		segments.push({ text: line.slice(lastIndex), isCode: false });
	return segments;
}
