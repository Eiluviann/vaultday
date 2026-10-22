import { linkKey } from './link-rewrite.ts';
import { classifyLines, mapOutsideInlineCode } from './markdown-regions.ts';
import { isNotePath, readNoteText, scanVault } from './vault-scan.ts';

export type UnresolvedLink = {
	relativePath: string;
	line: number;
	target: string;
};

const WIKILINK_PATTERN = /!?\[\[([^\]|#^]+)(?:[#^][^\]|]*)?(?:\|[^\]]*)?\]\]/g;

/** Every wikilink in prose or frontmatter whose target is not a file in the vault (Obsidian's basename or path resolution). */
export function verifyLinks(vaultRoot: string): UnresolvedLink[] {
	const entries = scanVault(vaultRoot).filter((entry) => entry.kind === 'file');
	const byBasename = new Set<string>();
	const byPath = new Set<string>();
	for (const entry of entries) {
		const basename = entry.relativePath.split('/').at(-1) ?? '';
		byBasename.add(linkKey(basename));
		byPath.add(linkKey(entry.relativePath));
		if (basename.endsWith('.md')) {
			byBasename.add(linkKey(basename.slice(0, -3)));
			byPath.add(linkKey(entry.relativePath.slice(0, -3)));
		}
	}
	const unresolved: UnresolvedLink[] = [];
	for (const entry of entries.filter((candidate) =>
		isNotePath(candidate.relativePath),
	)) {
		const lines = readNoteText(entry.absolutePath).split('\n');
		const regions = classifyLines(lines);
		lines.forEach((text, index) => {
			const region = regions[index]?.region;
			if (region !== 'prose' && region !== 'frontmatter') return;
			const collect = (segment: string) => {
				for (const match of segment.matchAll(WIKILINK_PATTERN)) {
					const target = (match[1] ?? '').trim();
					if (target.length === 0) continue;
					const resolved = target.includes('/')
						? byPath.has(linkKey(target))
						: byBasename.has(linkKey(target));
					if (!resolved)
						unresolved.push({
							relativePath: entry.relativePath,
							line: index + 1,
							target,
						});
				}
				return segment;
			};
			if (region === 'prose') mapOutsideInlineCode(text, collect);
			else collect(text);
		});
	}
	return unresolved;
}
