import { pathVariants } from './link-rewrite.ts';
import { isFenceLine } from './markdown-regions.ts';
import {
	currentRegions,
	currentTexts,
	type NoteDocument,
	replaceLineText,
} from './note-document.ts';
import type { PathMap } from './path-map.ts';

const QUERY_LANGUAGES = new Set(['dataview', 'tasks']);
const DATAVIEW_KEY_RENAMES: ReadonlyArray<readonly [RegExp, string]> = [
	[/\bNumber\b/g, 'number'],
	[/\bStatus\b/g, 'status'],
	[/\bTicket\b/g, 'ticket'],
];
const TASKS_PATH_CLAUSE =
	/^(\s*(?:path|folder|filename) (?:includes|does not include) )(.*)$/;

/**
 * Lowercases the renamed frontmatter keys inside dataview fences and retargets
 * moved paths quoted in dataview or named in Tasks `path includes` clauses.
 */
export function rewriteQueries(
	document: NoteDocument,
	pathMap: PathMap,
): number {
	const regions = currentRegions(document);
	const variants = pathVariants(pathMap, '');
	let rewritten = 0;
	currentTexts(document).forEach((text, index) => {
		const region = regions[index];
		if (
			region?.region !== 'fence' ||
			isFenceLine(text) ||
			!QUERY_LANGUAGES.has(region.fenceLanguage ?? '')
		)
			return;
		let next = text;
		if (region.fenceLanguage === 'dataview') {
			for (const [pattern, replacement] of DATAVIEW_KEY_RENAMES)
				next = next.replace(pattern, replacement);
			next = next.replace(
				/"([^"]*)"/g,
				(_whole, inner: string) => `"${retargetPath(inner, variants)}"`,
			);
		} else {
			const clause = TASKS_PATH_CLAUSE.exec(next);
			if (clause !== null)
				next = `${clause[1] ?? ''}${retargetPath(clause[2] ?? '', variants)}`;
		}
		if (next !== text) rewritten++;
		replaceLineText(document, index, next, 'query');
	});
	return rewritten;
}

function retargetPath(
	value: string,
	variants: Array<[string, string]>,
): string {
	for (const [from, to] of variants) {
		if (value === from) return to;
		if (value.startsWith(`${from}/`)) return `${to}${value.slice(from.length)}`;
	}
	return value;
}
