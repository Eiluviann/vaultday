import {
	mapInsideInlineCode,
	mapOutsideInlineCode,
} from './markdown-regions.ts';
import {
	currentRegions,
	currentTexts,
	type NoteDocument,
	replaceLineText,
} from './note-document.ts';
import type { PathMap } from './path-map.ts';
import {
	basenameWithoutExtension,
	isUnderDirectory,
	normalisePath,
	parentDirectory,
} from './paths.ts';

/** Link targets that change because a note was renamed or moved under a new basename. */
export type LinkRewriteTable = {
	/** lowercase NFC old basename (no extension) → new basename */
	byBasename: Map<string, string>;
	/** lowercase NFC old vault path without `.md` → new vault path without `.md` */
	byPath: Map<string, string>;
};

const WIKILINK_PATTERN =
	/(!?)\[\[([^\]|#^]+)((?:[#^][^\]|]*)?)((?:\|[^\]]*)?)\]\]/g;
const QUOTED_PATTERN = /"([^"\n]*)"/g;

export function buildLinkRewriteTable(
	pathMap: PathMap,
	notePaths: Iterable<string>,
): LinkRewriteTable {
	const table: LinkRewriteTable = { byBasename: new Map(), byPath: new Map() };
	for (const notePath of notePaths) {
		const target = pathMap.map(notePath);
		if (target === undefined || target === notePath || !target.endsWith('.md'))
			continue;
		const oldBasename = basenameWithoutExtension(notePath);
		const newBasename = basenameWithoutExtension(target);
		if (oldBasename !== newBasename)
			table.byBasename.set(linkKey(oldBasename), newBasename);
		table.byPath.set(linkKey(notePath.slice(0, -3)), target.slice(0, -3));
	}
	return table;
}

/** Rewrites `[[Old]]`, `[[Old|a]]`, `[[Old#h]]`, `![[Old]]` in prose and frontmatter lines (inline code untouched). */
export function rewriteWikilinks(
	document: NoteDocument,
	table: LinkRewriteTable,
): number {
	if (table.byBasename.size === 0 && table.byPath.size === 0) return 0;
	const regions = currentRegions(document);
	let rewritten = 0;
	currentTexts(document).forEach((text, index) => {
		const region = regions[index]?.region;
		if (region !== 'prose' && region !== 'frontmatter') return;
		const replaceLinks = (segment: string) =>
			segment.replace(
				WIKILINK_PATTERN,
				(
					whole,
					embed: string,
					target: string,
					anchor: string,
					alias: string,
				) => {
					const replacement = retarget(target, table);
					if (replacement === undefined) return whole;
					rewritten++;
					return `${embed}[[${replacement}${anchor}${alias}]]`;
				},
			);
		const next =
			region === 'prose'
				? mapOutsideInlineCode(text, replaceLinks)
				: replaceLinks(text);
		replaceLineText(document, index, next, 'link');
	});
	return rewritten;
}

/**
 * Rewrites moved paths quoted in prose: inside backticks and double quotes,
 * spelled vault-relative or relative to the note's folder.
 */
export function rewritePathStrings(
	document: NoteDocument,
	pathMap: PathMap,
	noteDirectory: string,
): number {
	const variants = pathVariants(pathMap, noteDirectory);
	if (variants.length === 0) return 0;
	const regions = currentRegions(document);
	let rewritten = 0;
	const replaceInSpan = (span: string) => {
		let result = span;
		for (const [from, to] of variants) {
			const pattern = new RegExp(
				`(^|[\\s"'(])${escapeRegExp(from)}(?=$|/|[\\s"')])`,
				'g',
			);
			result = result.replace(pattern, (_whole, lead: string) => {
				rewritten++;
				return `${lead}${to}`;
			});
		}
		return result;
	};
	currentTexts(document).forEach((text, index) => {
		if (regions[index]?.region !== 'prose') return;
		const withCode = mapInsideInlineCode(text, replaceInSpan);
		const withQuotes = withCode.replace(
			QUOTED_PATTERN,
			(_whole, inner: string) => `"${replaceInSpan(inner)}"`,
		);
		replaceLineText(document, index, withQuotes, 'link');
	});
	return rewritten;
}

/** Old → new spellings of moved paths, longest first, vault-relative and note-relative. */
export function pathVariants(
	pathMap: PathMap,
	noteDirectory: string,
): Array<[string, string]> {
	const variants: Array<[string, string]> = [];
	for (const mapping of pathMap.mappings) {
		if (mapping.to === undefined) continue;
		variants.push([mapping.from, mapping.to]);
		if (
			noteDirectory.length > 0 &&
			isUnderDirectory(mapping.from, noteDirectory) &&
			mapping.from !== noteDirectory
		) {
			const relativeFrom = mapping.from.slice(noteDirectory.length + 1);
			const relativeTo = isUnderDirectory(mapping.to, noteDirectory)
				? mapping.to.slice(noteDirectory.length + 1)
				: mapping.to;
			variants.push([relativeFrom, relativeTo]);
		}
	}
	return variants.sort((left, right) => right[0].length - left[0].length);
}

function retarget(
	rawTarget: string,
	table: LinkRewriteTable,
): string | undefined {
	const target = rawTarget.trim();
	if (target.length === 0) return undefined;
	const hasExtension = target.endsWith('.md');
	const bare = hasExtension ? target.slice(0, -3) : target;
	const replacement = bare.includes('/')
		? table.byPath.get(linkKey(bare))
		: table.byBasename.get(linkKey(bare));
	if (replacement === undefined) return undefined;
	return hasExtension ? `${replacement}.md` : replacement;
}

export function linkKey(value: string): string {
	return normalisePath(value).toLowerCase();
}

export function noteDirectoryOf(relativePath: string): string {
	return parentDirectory(relativePath);
}

function escapeRegExp(value: string): string {
	return value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}
