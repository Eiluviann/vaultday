import { readdirSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { addDays, type PlaygroundCalendar } from './calendar.ts';
import { doneMoments, type PlaygroundFile, renderDayLogs } from './day-log.ts';
import { mockStudioDayLogs, NEW_STORIES } from './mock-studio-plan.ts';
import { renderStoryNote, type StorySource } from './notes.ts';
import {
	adaptFixtureNote,
	holdNoteToCutoff,
	type NoteCutoff,
} from './vault-text.ts';

/** One generated workspace root: its folder name under the playground and its files. */
export type WorkspaceBuild = {
	folderName: string;
	files: PlaygroundFile[];
};

export const MOCK_STUDIO_FOLDER = 'Mock Studio';

const LOG_FOLDER = 'Log';
const TRACKER: StorySource = {
	host: 'tracker.example.test',
	ticketUrl: (ticket) => `https://tracker.example.test/browse/${ticket}`,
};
/**
 * The fixture's folders the playground writes itself: settings, the logs, and
 * `.obsidian`, which the writer only adds when missing so a reset keeps
 * Obsidian's own state.
 */
const SKIPPED_FIXTURE_PATHS = new Set(['.vaultday', '.obsidian', LOG_FOLDER]);

/**
 * The work workspace: the mock vault's client "Mock Studio" with its stories
 * moved to the focus week, four more stories, two weeks of day logs with
 * sessions, decisions and blockers, and settings that turn on Hours and the
 * week close-out (hourly billing, a Monday deadline).
 *
 * @param fixtureRoot `core/fixtures/mock-vault`.
 */
export function buildMockStudio(
	fixtureRoot: string,
	calendar: PlaygroundCalendar,
): WorkspaceBuild {
	const dateOf = (offset: number) => addDays(calendar.focusMonday, offset);
	const days = mockStudioDayLogs().map((day) => ({
		date: dateOf(day.day),
		entries: day.entries,
	}));
	const cutoff: NoteCutoff = {
		today: calendar.today,
		now: calendar.now,
		doneTimes: doneMoments(days),
	};
	const storyFiles = NEW_STORIES.map((story) => ({
		path: `Projects/${story.projectFolder}/${story.number} ${story.title}/${story.number} ${story.title}.md`,
		content: holdNoteToCutoff(renderStoryNote(story, TRACKER, dateOf), cutoff),
	}));
	const logFiles = renderDayLogs(days, {
		logFolder: LOG_FOLDER,
		today: calendar.today,
		now: calendar.now,
		firstSessionNumber: 101,
	});
	return {
		folderName: MOCK_STUDIO_FOLDER,
		files: [
			{
				path: '.vaultday/settings.json',
				content: mockStudioSettings(fixtureRoot),
			},
			...readFixtureNotes(fixtureRoot, calendar.fixtureShiftDays, cutoff),
			...storyFiles,
			...logFiles,
		],
	};
}

function mockStudioSettings(fixtureRoot: string): string {
	const fixtureSettings: unknown = JSON.parse(
		readFileSync(path.join(fixtureRoot, '.vaultday/settings.json'), 'utf8'),
	);
	if (!isRecord(fixtureSettings))
		throw new Error('the mock vault fixture holds no settings object');
	// The fixture's unknown key tests forward compatibility; a playground has no use for it.
	const { x_future_setting: _unusedFutureSetting, ...settings } =
		fixtureSettings;
	const playgroundSettings = {
		...settings,
		// The fixture's "teal" is no workspace color of this build and would read as graphite.
		accent: 'copper',
		// settings.json keys are snake_case; entries keep them out of the camelCase lint.
		...Object.fromEntries([
			[
				'billing',
				Object.fromEntries([
					['model', 'hourly'],
					['rounding_minutes', 15],
					['billable_default', true],
				]),
			],
			['log_deadline', { weekday: 'monday', time: '12:00' }],
			['leave_source', 'Personal'],
		]),
	};
	return `${JSON.stringify(playgroundSettings, undefined, 2)}\n`;
}

function readFixtureNotes(
	fixtureRoot: string,
	shiftDays: number,
	cutoff: NoteCutoff,
): PlaygroundFile[] {
	return listFixtureFiles(fixtureRoot, '').map((relativePath) => {
		const text = readFileSync(path.join(fixtureRoot, relativePath), 'utf8');
		const content = relativePath.endsWith('.md')
			? adaptFixtureNote(relativePath, text, shiftDays, cutoff)
			: text;
		return { path: relativePath, content };
	});
}

function listFixtureFiles(root: string, relativeFolder: string): string[] {
	return readdirSync(path.join(root, relativeFolder), { withFileTypes: true })
		.filter(
			(entry) =>
				relativeFolder !== '' || !SKIPPED_FIXTURE_PATHS.has(entry.name),
		)
		.filter((entry) => entry.name !== '.DS_Store')
		.flatMap((entry) => {
			const relativePath =
				relativeFolder === '' ? entry.name : `${relativeFolder}/${entry.name}`;
			return entry.isDirectory()
				? listFixtureFiles(root, relativePath)
				: [relativePath];
		})
		.sort();
}

function isRecord(value: unknown): value is Record<string, unknown> {
	return typeof value === 'object' && value !== null && !Array.isArray(value);
}
