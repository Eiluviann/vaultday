import { resolveWorkspace, type WorkspaceHandle } from '@vaultday/core-node';
import { type Failure, failureFromThrown } from './failure.ts';
import { err, ok, type Result } from './result.ts';

/** Runs a call into the native binding and turns what it throws into a `Failure`. */
export function callCore<T>(action: () => T): Result<T, Failure> {
	try {
		return ok(action());
	} catch (thrown) {
		return err(failureFromThrown(thrown));
	}
}

/**
 * Finds the workspace holding `directory`: the nearest `.vaultday/` at or
 * above it wins. No symlink is followed, neither one the shell
 * went through to get here nor one in the workspace root: a synced vault
 * carries the links of whoever synced it, and the CLI has no per-device
 * record of the folders the user linked, so it never sees linked folders.
 * Outside a workspace the failure says what to do about it.
 */
export function resolveWorkspaceFrom(
	directory: string,
): Result<WorkspaceHandle, Failure> {
	const result = callCore(() => resolveWorkspace(directory));
	if (result.ok || result.error.kind !== 'not-a-workspace') return result;
	return err({
		kind: 'not-a-workspace',
		message: `${result.error.message}. Run vaultday inside a Vaultday workspace (a folder with .vaultday/settings.json).`,
	});
}
