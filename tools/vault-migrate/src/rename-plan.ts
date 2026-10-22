import {
	basenameWithoutExtension,
	joinRelative,
	parentDirectory,
} from './paths.ts';
import { err, ok, type Result } from './result.ts';
import {
	SHORT_TITLE_OVERRIDES,
	storyFolderName,
} from './rules/short-titles.ts';

export type StoryForRename = { relativePath: string; number: string };

export type RenameEntry = {
	number: string;
	oldTitle: string;
	newTitle: string;
	fromDirectory: string;
	toDirectory: string;
	fromNote: string;
	toNote: string;
};

/**
 * Plans `STRY… Short title` folder+note renames for numbered folder notes.
 * Unnumbered stories and notes already carrying their number stay put.
 */
export function planRenames(
	stories: StoryForRename[],
	overrides: ReadonlyMap<string, string> = SHORT_TITLE_OVERRIDES,
): Result<RenameEntry[]> {
	const entries: RenameEntry[] = [];
	for (const story of stories) {
		if (story.number.length === 0) continue;
		const oldTitle = basenameWithoutExtension(story.relativePath);
		const fromDirectory = parentDirectory(story.relativePath);
		if (basenameWithoutExtension(fromDirectory) !== oldTitle) continue;
		if (oldTitle.startsWith(`${story.number} `)) continue;
		const newTitle = storyFolderName(story.number, oldTitle, overrides);
		const toDirectory = joinRelative(parentDirectory(fromDirectory), newTitle);
		entries.push({
			number: story.number,
			oldTitle,
			newTitle,
			fromDirectory,
			toDirectory,
			fromNote: story.relativePath,
			toNote: `${toDirectory}/${newTitle}.md`,
		});
	}
	const seen = new Map<string, string>();
	for (const entry of entries) {
		const clash = seen.get(entry.toDirectory.toLowerCase());
		if (clash !== undefined)
			return err(
				`rename collision: ${clash} and ${entry.fromDirectory} both become ${entry.toDirectory}`,
			);
		seen.set(entry.toDirectory.toLowerCase(), entry.fromDirectory);
	}
	return ok(entries);
}
