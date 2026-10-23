import {
	cpSync,
	mkdtempSync,
	readdirSync,
	readFileSync,
	rmSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { afterEach } from 'vitest';
import { type CliContext, runCli } from '../src/cli.ts';

const repositoryRoot = path.resolve(
	path.dirname(fileURLToPath(import.meta.url)),
	'..',
	'..',
);
export const MOCK_VAULT = path.join(
	repositoryRoot,
	'core',
	'fixtures',
	'mock-vault',
);
export const WEB_142_NOTE =
	'Projects/Website/WEB-142 Checkout redesign/WEB-142 Checkout redesign.md';

const temporaryDirectories: string[] = [];

afterEach(() => {
	for (const directory of temporaryDirectories.splice(0)) {
		rmSync(directory, { recursive: true, force: true });
	}
});

/** A copy of the mock vault in a temporary directory; the fixture itself is never written to. */
export function copyMockVault(): string {
	const directory = mkdtempSync(path.join(tmpdir(), 'vaultday-cli-'));
	temporaryDirectories.push(directory);
	const copy = path.join(directory, 'vault');
	cpSync(MOCK_VAULT, copy, { recursive: true });
	return copy;
}

/** An empty directory outside any workspace. */
export function emptyDirectory(): string {
	const directory = mkdtempSync(path.join(tmpdir(), 'vaultday-empty-'));
	temporaryDirectories.push(directory);
	return directory;
}

export type CliRun = {
	readonly code: number;
	readonly out: string;
	readonly error: string;
};

/** Runs the CLI in-process from `workingDirectory` with the given environment. */
export async function runVaultday(
	argv: readonly string[],
	workingDirectory: string,
	environment: CliContext['environment'] = {},
): Promise<CliRun> {
	let out = '';
	let error = '';
	const code = await runCli(argv, {
		workingDirectory,
		environment,
		writeOut: (text) => {
			out += text;
		},
		writeError: (text) => {
			error += text;
		},
	});
	return { code, out, error };
}

/** Every file below `root` as relative path to text. */
export function snapshot(root: string): ReadonlyMap<string, string> {
	const files = new Map<string, string>();
	const visit = (directory: string): void => {
		for (const entry of readdirSync(directory, { withFileTypes: true })) {
			const entryPath = path.join(directory, entry.name);
			if (entry.isDirectory()) visit(entryPath);
			else
				files.set(
					path.relative(root, entryPath),
					readFileSync(entryPath, 'utf8'),
				);
		}
	};
	visit(root);
	return files;
}

export type FileChange = {
	readonly path: string;
	readonly removed: readonly string[];
	readonly added: readonly string[];
};

/** Which lines each changed file lost and gained, after trimming the common head and tail. */
export function diffSnapshots(
	before: ReadonlyMap<string, string>,
	after: ReadonlyMap<string, string>,
): FileChange[] {
	const paths = [...new Set([...before.keys(), ...after.keys()])].sort();
	return paths.flatMap((filePath) => {
		const oldLines = (before.get(filePath) ?? '').split('\n');
		const newLines = (after.get(filePath) ?? '').split('\n');
		if (before.get(filePath) === after.get(filePath)) return [];
		let head = 0;
		while (
			head < oldLines.length &&
			head < newLines.length &&
			oldLines[head] === newLines[head]
		)
			head += 1;
		let tail = 0;
		while (
			tail < oldLines.length - head &&
			tail < newLines.length - head &&
			oldLines[oldLines.length - 1 - tail] ===
				newLines[newLines.length - 1 - tail]
		)
			tail += 1;
		return [
			{
				path: filePath,
				removed: oldLines.slice(head, oldLines.length - tail),
				added: newLines.slice(head, newLines.length - tail),
			},
		];
	});
}

/** The environment of a session whose text a model wrote: `AI_PROVENANCE_AUTHOR` names the model. */
export function modelSession(author: string): CliContext['environment'] {
	return Object.fromEntries([['AI_PROVENANCE_AUTHOR', author]]);
}
