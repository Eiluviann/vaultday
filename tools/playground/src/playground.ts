import path from 'node:path';
import type { PlaygroundCalendar } from './calendar.ts';
import { LAUNCHER_NAME, launcherScript } from './launcher.ts';
import { buildMockStudio, MOCK_STUDIO_FOLDER } from './mock-studio.ts';
import { buildPersonal, PERSONAL_FOLDER } from './personal.ts';
import type { PlaygroundPlan } from './write-playground.ts';

/** Where the playground goes, the dates it is built around, and the fixtures it derives from. */
export type PlaygroundInput = {
	outDirectory: string;
	calendar: PlaygroundCalendar;
	/** `core/fixtures`. */
	fixturesRoot: string;
};

/** The work and life roots and the launcher, ready for `writePlayground`. */
export function buildPlaygroundPlan(input: PlaygroundInput): PlaygroundPlan {
	return {
		outDirectory: input.outDirectory,
		workspaces: [
			buildMockStudio(
				path.join(input.fixturesRoot, 'mock-vault'),
				input.calendar,
			),
			buildPersonal(
				path.join(input.fixturesRoot, 'two-workspaces'),
				input.calendar,
			),
		],
		launcher: {
			name: LAUNCHER_NAME,
			content: launcherScript({
				work: MOCK_STUDIO_FOLDER,
				life: PERSONAL_FOLDER,
			}),
		},
	};
}
