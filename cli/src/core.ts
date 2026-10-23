import { existsSync, lstatSync, realpathSync } from 'node:fs';
import path from 'node:path';
import { resolveWorkspace, type WorkspaceHandle } from '@vaultday/core-node';
import { type Failure, failureFromThrown } from './failure.ts';
import { err, ok, type Result } from './result.ts';

const SHELL_DIRECTORY_ENVIRONMENT_KEY = 'PWD';

/** Runs a call into the native binding and turns what it throws into a `Failure`. */
export function callCore<T>(action: () => T): Result<T, Failure> {
	try {
		return ok(action());
	} catch (thrown) {
		return err(failureFromThrown(thrown));
	}
}

/**
 * The directory to resolve the workspace from: the shell's logical `PWD` when
 * it names the same folder as the real working directory, so a folder linked
 * into a workspace root resolves to that workspace; the real one otherwise.
 */
export function logicalWorkingDirectory(
	realDirectory: string,
	environment: Readonly<Record<string, string | undefined>>,
): string {
	const shellDirectory = environment[SHELL_DIRECTORY_ENVIRONMENT_KEY];
	if (shellDirectory === undefined || shellDirectory === realDirectory) {
		return realDirectory;
	}
	try {
		return realpathSync(shellDirectory) === realpathSync(realDirectory)
			? shellDirectory
			: realDirectory;
	} catch {
		return realDirectory;
	}
}

/**
 * Finds the workspace holding `directory`: the nearest `.vaultday/` at or
 * above it wins. A directory in no workspace may still be a folder linked
 * into one: when the shell's logical `PWD` names it through a link that sits
 * directly in a workspace root, that root's workspace is taken. Outside a
 * workspace the failure says what to do about it.
 */
export function resolveWorkspaceFrom(
	directory: string,
	environment: Readonly<Record<string, string | undefined>> = {},
): Result<WorkspaceHandle, Failure> {
	const result = callCore(() => resolveWorkspace(directory));
	if (result.ok || result.error.kind !== 'not-a-workspace') return result;
	const shellDirectory = logicalWorkingDirectory(directory, environment);
	if (shellDirectory !== directory && isThroughRootLink(shellDirectory)) {
		const linked = callCore(() => resolveWorkspace(shellDirectory));
		if (linked.ok || linked.error.kind !== 'not-a-workspace') return linked;
	}
	return err({
		kind: 'not-a-workspace',
		message: `${result.error.message}. Run vaultday inside a Vaultday workspace (a folder with .vaultday/settings.json).`,
	});
}

/** Whether the nearest symlink on `directory`'s path sits directly in a workspace root. */
function isThroughRootLink(directory: string): boolean {
	for (
		let current = directory;
		path.dirname(current) !== current;
		current = path.dirname(current)
	) {
		if (isSymbolicLink(current)) {
			return existsSync(
				path.join(path.dirname(current), '.vaultday', 'settings.json'),
			);
		}
	}
	return false;
}

function isSymbolicLink(entry: string): boolean {
	try {
		return lstatSync(entry).isSymbolicLink();
	} catch {
		return false;
	}
}
