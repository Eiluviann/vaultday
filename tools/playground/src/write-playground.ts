import {
	chmodSync,
	existsSync,
	mkdirSync,
	readdirSync,
	renameSync,
	rmSync,
	writeFileSync,
} from 'node:fs';
import path from 'node:path';
import type { WorkspaceBuild } from './mock-studio.ts';

export type Result<T, E = string> =
	| { ok: true; value: T }
	| { ok: false; error: E };

/** Everything one run writes under the playground folder. */
export type PlaygroundPlan = {
	outDirectory: string;
	/** Where an earlier, single-root playground is moved to before the new layout goes in. */
	archiveDirectory: string;
	workspaces: WorkspaceBuild[];
	launcher: { name: string; content: string };
};

/** What a run did, for the summary it prints. */
export type PlaygroundWriteReport = {
	archivedTo?: string;
	keptAppState: string[];
	writtenFiles: number;
};

/** A root holding this file was written by the generator; a reset may replace its notes. */
export const PLAYGROUND_MARKER = '.vaultday-playground';

/**
 * Entries a reset keeps in a root: the app's own state for the workspace
 * (`.vaultday/sync`, its id and change files, so the app still knows the
 * workspace) and Obsidian's state. `settings.json` is rewritten.
 */
const KEPT_ROOT_ENTRIES = new Set(['.vaultday', '.obsidian']);
const OBSIDIAN_APP_SETTINGS = `${JSON.stringify({ useTab: true, tabSize: 4 }, undefined, '\t')}\n`;

/**
 * Writes the playground: archives an earlier single-root layout, clears each
 * generated root (refusing a folder the generator did not write) and writes
 * the notes, settings and launcher.
 *
 * @returns The report, or why a root was left alone.
 */
export function writePlayground(
	plan: PlaygroundPlan,
	stamp: string,
): Result<PlaygroundWriteReport> {
	const foreignRoot = plan.workspaces
		.map((workspace) => path.join(plan.outDirectory, workspace.folderName))
		.find(isForeignFolder);
	if (foreignRoot !== undefined) {
		return {
			ok: false,
			error: `${foreignRoot} holds files the playground did not write (no ${PLAYGROUND_MARKER}); move it away first`,
		};
	}
	mkdirSync(plan.outDirectory, { recursive: true });
	const archivedTo = archiveSingleRootLayout(plan, stamp);
	const keptAppState = plan.workspaces
		.filter((workspace) =>
			clearRoot(path.join(plan.outDirectory, workspace.folderName)),
		)
		.map((workspace) => workspace.folderName);
	const writtenFiles = plan.workspaces.reduce(
		(count, workspace) => count + writeWorkspace(plan.outDirectory, workspace),
		0,
	);
	const launcherPath = path.join(plan.outDirectory, plan.launcher.name);
	writeFileSync(launcherPath, plan.launcher.content);
	chmodSync(launcherPath, 0o755);
	return {
		ok: true,
		value: {
			...(archivedTo === undefined ? {} : { archivedTo }),
			keptAppState,
			writtenFiles: writtenFiles + 1,
		},
	};
}

/**
 * The playground used to be one root (`~/Vaultday-playground/.vaultday`).
 * Two roots cannot sit inside a third, so that layout moves aside, whole,
 * before the new one is written; nothing of it is deleted.
 */
function archiveSingleRootLayout(
	plan: PlaygroundPlan,
	stamp: string,
): string | undefined {
	if (!existsSync(path.join(plan.outDirectory, '.vaultday'))) return undefined;
	const ownNames = new Set([
		...plan.workspaces.map((workspace) => workspace.folderName),
		plan.launcher.name,
	]);
	const destination = path.join(plan.archiveDirectory, stamp);
	mkdirSync(destination, { recursive: true });
	for (const name of readdirSync(plan.outDirectory)) {
		if (!ownNames.has(name))
			renameSync(
				path.join(plan.outDirectory, name),
				path.join(destination, name),
			);
	}
	return destination;
}

/** Empties a generated root but for the kept entries; `true` when the app's state for it was kept. */
function clearRoot(root: string): boolean {
	if (!existsSync(root)) return false;
	for (const name of readdirSync(root)) {
		if (!KEPT_ROOT_ENTRIES.has(name))
			rmSync(path.join(root, name), { recursive: true, force: true });
	}
	return existsSync(path.join(root, '.vaultday', 'sync'));
}

function writeWorkspace(
	outDirectory: string,
	workspace: WorkspaceBuild,
): number {
	const root = path.join(outDirectory, workspace.folderName);
	for (const file of workspace.files) {
		const target = path.join(root, file.path);
		mkdirSync(path.dirname(target), { recursive: true });
		writeFileSync(target, file.content);
	}
	const obsidianSettings = path.join(root, '.obsidian', 'app.json');
	const extraFiles = [
		writeIfMissing(obsidianSettings, OBSIDIAN_APP_SETTINGS),
		writeIfMissing(
			path.join(root, PLAYGROUND_MARKER),
			'Written by tools/playground in the vaultday repository; a reset replaces every note in this folder.\n',
		),
	];
	return workspace.files.length + extraFiles.filter(Boolean).length;
}

function writeIfMissing(target: string, content: string): boolean {
	if (existsSync(target)) return false;
	mkdirSync(path.dirname(target), { recursive: true });
	writeFileSync(target, content);
	return true;
}

function isForeignFolder(folder: string): boolean {
	return (
		existsSync(folder) &&
		readdirSync(folder).length > 0 &&
		!existsSync(path.join(folder, PLAYGROUND_MARKER))
	);
}
