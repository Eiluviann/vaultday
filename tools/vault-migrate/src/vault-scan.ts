import { readdirSync, readFileSync, statSync } from 'node:fs';
import path from 'node:path';
import {
	basenameWithoutExtension,
	hasPathSegment,
	normalisePath,
} from './paths.ts';

export type VaultEntry = {
	relativePath: string;
	absolutePath: string;
	kind: 'file' | 'directory';
};

/** Same exclusions as the provenance hook's listVaultNotes, plus the Mac-only code folder. */
export const SKIPPED_DIRECTORY_NAMES = new Set([
	'.obsidian',
	'.trash',
	'.git',
	'.claude',
	'.claude-memory-archive',
	'node_modules',
	'dist',
	'sn-portal-mock',
]);
export const SKIPPED_NOTE_BASENAMES = new Set([
	'CLAUDE.md',
	'AGENTS.md',
	'AI provenance.md',
	'ai-provenance.md',
	'Review queue.md',
]);
export const TEMPLATES_DIRECTORY = '_templates';
export const AGENT_DIRECTORY = '_agent';
export const LOG_DIRECTORY = 'Log';

/** Every file and directory of the vault outside the skipped folders, sorted by relative path. */
export function scanVault(vaultRoot: string): VaultEntry[] {
	const entries: VaultEntry[] = [];
	walk('');
	return entries.sort((left, right) =>
		left.relativePath.localeCompare(right.relativePath),
	);

	function walk(relativeDirectory: string): void {
		const absoluteDirectory = path.join(vaultRoot, relativeDirectory);
		for (const dirent of readdirSync(absoluteDirectory, {
			withFileTypes: true,
		})) {
			if (dirent.name === '.DS_Store') continue;
			const relativePath = normalisePath(
				relativeDirectory.length === 0
					? dirent.name
					: `${relativeDirectory}/${dirent.name}`,
			);
			const absolutePath = path.join(absoluteDirectory, dirent.name);
			if (dirent.isDirectory()) {
				if (SKIPPED_DIRECTORY_NAMES.has(dirent.name)) continue;
				entries.push({ relativePath, absolutePath, kind: 'directory' });
				walk(relativePath);
			} else if (dirent.isFile()) {
				entries.push({ relativePath, absolutePath, kind: 'file' });
			}
		}
	}
}

/** A markdown note the convention applies to (not an agent-rules or generated file). */
export function isNotePath(relativePath: string): boolean {
	return (
		relativePath.endsWith('.md') &&
		!SKIPPED_NOTE_BASENAMES.has(path.posix.basename(relativePath))
	);
}

export function isTemplatePath(relativePath: string): boolean {
	return hasPathSegment(relativePath, TEMPLATES_DIRECTORY);
}

/** Notes whose task lines never get ids: templates, agent material and day logs. */
export function isTaskIdExemptPath(relativePath: string): boolean {
	return (
		isTemplatePath(relativePath) ||
		hasPathSegment(relativePath, AGENT_DIRECTORY) ||
		hasPathSegment(relativePath, LOG_DIRECTORY)
	);
}

/** True when the note shares its name with its parent folder (hub or story note). */
export function isFolderNote(relativePath: string): boolean {
	const segments = relativePath.split('/');
	const parent = segments.at(-2);
	return (
		parent !== undefined && basenameWithoutExtension(relativePath) === parent
	);
}

export function readNoteText(absolutePath: string): string {
	return readFileSync(absolutePath, 'utf8');
}

export function fileSizeInBytes(absolutePath: string): number {
	return statSync(absolutePath).size;
}
