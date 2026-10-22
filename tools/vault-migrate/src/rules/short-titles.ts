export const SHORT_TITLE_MAX_LENGTH = 40;

/** Story number → short title decided with the user; unlisted stories keep their title (shortened only if too long). */
export const SHORT_TITLE_OVERRIDES: ReadonlyMap<string, string> = new Map([
	['STRY0681010', 'CSAT Portfolio hierarchy'],
	['STRY0602768', 'CSAT rating 5 to 2'],
	['STRY0602776', 'Open Feedback Portfolio hierarchy'],
	['STRY0602779', 'Open Feedback form new categories'],
	['STRY0602785', 'Issue Log Portfolio hierarchy'],
	['STRY0602788', 'Issue Log auto-assignment'],
	['STRY0602796', 'Issue Log on-behalf in My Issues'],
	['STRY0602806', 'Issue Log in Workspaces'],
]);

/** Strips parentheticals and cuts at a word boundary to the maximum length. */
export function shortenTitle(
	title: string,
	maxLength: number = SHORT_TITLE_MAX_LENGTH,
): string {
	const withoutParentheticals = title
		.replace(/\s*\([^)]*\)/g, '')
		.replace(/\s+/g, ' ')
		.trim();
	if (withoutParentheticals.length <= maxLength) return withoutParentheticals;
	const cut = withoutParentheticals.slice(0, maxLength + 1);
	const lastSpace = cut.lastIndexOf(' ');
	const truncated =
		lastSpace > 0 ? cut.slice(0, lastSpace) : cut.slice(0, maxLength);
	return truncated.replace(/[\s,.;:–-]+$/, '');
}

export function storyFolderName(
	number: string,
	title: string,
	overrides: ReadonlyMap<string, string> = SHORT_TITLE_OVERRIDES,
): string {
	return `${number} ${overrides.get(number) ?? shortenTitle(title)}`;
}
