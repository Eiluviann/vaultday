import {
	chmodSync,
	existsSync,
	mkdirSync,
	readdirSync,
	rmSync,
	statSync,
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
	workspaces: WorkspaceBuild[];
	launcher: { name: string; content: string };
};

/** What a run did, for the summary it prints. */
export type PlaygroundWriteReport = {
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
/** The fixtures' Obsidian settings, written only where a root has none. */
const OBSIDIAN_APP_SETTINGS = `${JSON.stringify({ useTab: true, tabSize: 4 }, undefined, 2)}\n`;
const MARKER_TEXT =
	'Written by tools/playground in the vaultday repository; a reset replaces every note in this folder.\n';

/**
 * Writes the playground: clears each generated root (keeping the app's and
 * Obsidian's state) and writes the notes, settings and launcher. Nothing is
 * touched when the folder is or lies in a workspace root, or when a root
 * holds files the generator did not write.
 *
 * @returns The report, or why nothing (or not everything) was written.
 */
export function writePlayground(
	plan: PlaygroundPlan,
): Result<PlaygroundWriteReport> {
	try {
		const refusal = refusalFor(plan);
		if (refusal !== undefined) return { ok: false, error: refusal };
		mkdirSync(plan.outDirectory, { recursive: true });
		const keptAppState = plan.workspaces
			.filter((workspace) =>
				clearRoot(path.join(plan.outDirectory, workspace.folderName)),
			)
			.map((workspace) => workspace.folderName);
		const writtenFiles = plan.workspaces.reduce(
			(count, workspace) =>
				count + writeWorkspace(plan.outDirectory, workspace),
			0,
		);
		const launcherPath = path.join(plan.outDirectory, plan.launcher.name);
		writeFileSync(launcherPath, plan.launcher.content);
		chmodSync(launcherPath, 0o755);
		return {
			ok: true,
			value: { keptAppState, writtenFiles: writtenFiles + 1 },
		};
	} catch (error) {
		const reason = error instanceof Error ? error.message : String(error);
		return {
			ok: false,
			error: `Writing the playground stopped part way (${reason}); fix that and run it again`,
		};
	}
}

/** Why the plan may not be written, checked before anything changes. */
function refusalFor(plan: PlaygroundPlan): string | undefined {
	const workspaceRoot = enclosingWorkspaceRoot(plan.outDirectory);
	if (workspaceRoot !== undefined) {
		return `${plan.outDirectory} is or lies in the workspace root ${workspaceRoot}; the playground goes in a folder of its own`;
	}
	for (const workspace of plan.workspaces) {
		const root = path.join(plan.outDirectory, workspace.folderName);
		if (!existsSync(root)) continue;
		if (!statSync(root).isDirectory())
			return `${root} is a file, not a folder; move it away first`;
		if (
			readdirSync(root).length > 0 &&
			!existsSync(path.join(root, PLAYGROUND_MARKER))
		) {
			return `${root} holds files the playground did not write (no ${PLAYGROUND_MARKER}); move it away first`;
		}
	}
	return undefined;
}

/** The nearest folder at or above `folder` that holds `.vaultday`, if any. */
function enclosingWorkspaceRoot(folder: string): string | undefined {
	let current = path.resolve(folder);
	for (;;) {
		if (existsSync(path.join(current, '.vaultday'))) return current;
		const parent = path.dirname(current);
		if (parent === current) return undefined;
		current = parent;
	}
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

/** Writes one root, marker first, so a run that fails part way leaves a root the next run may reset. */
function writeWorkspace(
	outDirectory: string,
	workspace: WorkspaceBuild,
): number {
	const root = path.join(outDirectory, workspace.folderName);
	mkdirSync(root, { recursive: true });
	writeFileSync(path.join(root, PLAYGROUND_MARKER), MARKER_TEXT);
	for (const file of workspace.files) {
		const target = path.join(root, file.path);
		mkdirSync(path.dirname(target), { recursive: true });
		writeFileSync(target, file.content);
	}
	const hasWrittenObsidianSettings = writeIfMissing(
		path.join(root, '.obsidian', 'app.json'),
		OBSIDIAN_APP_SETTINGS,
	);
	return workspace.files.length + 1 + (hasWrittenObsidianSettings ? 1 : 0);
}

function writeIfMissing(target: string, content: string): boolean {
	if (existsSync(target)) return false;
	mkdirSync(path.dirname(target), { recursive: true });
	writeFileSync(target, content);
	return true;
}
