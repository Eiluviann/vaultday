import { createHash } from 'node:crypto';
import {
	cpSync,
	existsSync,
	mkdirSync,
	readdirSync,
	renameSync,
	rmSync,
	statSync,
	writeFileSync,
} from 'node:fs';
import path from 'node:path';
import type { IdAllocator } from './id-allocator.ts';
import type { MigrationPlan } from './migration.ts';
import { isSameCaseInsensitive } from './paths.ts';
import { err, ok, type Result } from './result.ts';

export type ApplyManifest = {
	vault: string;
	appliedAt: string;
	renames: Array<{ from: string; to: string }>;
	moves: Array<{ from: string; to: string }>;
	deletes: Array<{ from: string; to: string }>;
	files: Array<{
		from: string;
		to: string;
		beforeSha256: string;
		afterSha256: string;
	}>;
	taskIds: string[];
	idStatePath: string;
};

export type ApplyOptions = {
	stateDirectory: string;
	allocator: IdAllocator;
	now?: Date;
};

/**
 * Writes the plan to disk: folder renames, moves, deletes (to `.trash/`),
 * then note contents at their final paths; prunes emptied folders, persists
 * the id counter and saves a manifest for rollback.
 */
export function applyPlan(
	plan: MigrationPlan,
	options: ApplyOptions,
): Result<ApplyManifest> {
	const preflight = checkDestinations(plan);
	if (!preflight.ok) return preflight;
	const absolute = (relativePath: string) =>
		path.join(plan.vaultRoot, relativePath);
	const manifest: ApplyManifest = {
		vault: plan.vaultRoot,
		appliedAt: (options.now ?? new Date()).toISOString(),
		renames: [],
		moves: [],
		deletes: [],
		files: [],
		taskIds: [...plan.allocatedIds],
		idStatePath: options.allocator.statePath,
	};
	const sourceParents = new Set<string>();

	for (const rename of plan.renames) {
		renameWithCaseGuard(
			absolute(rename.fromDirectory),
			absolute(rename.toDirectory),
		);
		renameSync(
			absolute(`${rename.toDirectory}/${rename.oldTitle}.md`),
			absolute(rename.toNote),
		);
		manifest.renames.push({
			from: rename.fromDirectory,
			to: rename.toDirectory,
		});
	}
	for (const move of plan.moves) {
		const destination = move.externalPath ?? absolute(move.to ?? '');
		const source = absolute(move.from);
		sourceParents.add(path.dirname(source));
		if (
			move.kind === 'directory' &&
			existsSync(destination) &&
			readdirSync(source).length === 0
		) {
			rmSync(source, { recursive: true });
		} else {
			mkdirSync(path.dirname(destination), { recursive: true });
			moveAcrossDevices(source, destination);
		}
		manifest.moves.push({ from: move.from, to: destination });
	}
	for (const entry of plan.deletes) {
		const source = absolute(entry.path);
		sourceParents.add(path.dirname(source));
		const trashPath = uniquePath(
			path.join(plan.vaultRoot, '.trash', path.basename(entry.path)),
		);
		mkdirSync(path.dirname(trashPath), { recursive: true });
		renameSync(source, trashPath);
		manifest.deletes.push({
			from: entry.path,
			to: path.relative(plan.vaultRoot, trashPath),
		});
	}
	for (const change of plan.changes) {
		if (change.before === change.after) continue;
		const target =
			change.externalPath ?? absolute(change.toPath ?? change.fromPath);
		writeAtomically(target, change.after);
		manifest.files.push({
			from: change.fromPath,
			to: target,
			beforeSha256: sha256(change.before),
			afterSha256: sha256(change.after),
		});
	}
	for (const parent of sourceParents)
		pruneEmptyDirectories(parent, plan.vaultRoot);
	options.allocator.persist();
	mkdirSync(options.stateDirectory, { recursive: true });
	const manifestPath = path.join(
		options.stateDirectory,
		`${path.basename(plan.vaultRoot)}.apply-manifest.${manifest.appliedAt.replace(/[:.]/g, '-')}.json`,
	);
	writeFileSync(manifestPath, `${JSON.stringify(manifest, null, '\t')}\n`);
	return ok(manifest);
}

function checkDestinations(plan: MigrationPlan): Result<undefined> {
	const absolute = (relativePath: string) =>
		path.join(plan.vaultRoot, relativePath);
	for (const rename of plan.renames) {
		if (
			existsSync(absolute(rename.toDirectory)) &&
			!isSameCaseInsensitive(rename.fromDirectory, rename.toDirectory)
		) {
			return err(`rename destination exists: ${rename.toDirectory}`);
		}
	}
	for (const move of plan.moves) {
		const destination = move.externalPath ?? absolute(move.to ?? '');
		if (!existsSync(destination)) continue;
		if (
			move.kind === 'directory' &&
			statSync(destination).isDirectory() &&
			readdirSync(absolute(move.from)).length === 0
		)
			continue;
		return err(`move destination exists: ${destination}`);
	}
	return ok(undefined);
}

function renameWithCaseGuard(from: string, to: string): void {
	if (from === to) return;
	if (isSameCaseInsensitive(from, to)) {
		const temporary = `${to}.rename-tmp`;
		renameSync(from, temporary);
		renameSync(temporary, to);
		return;
	}
	renameSync(from, to);
}

function moveAcrossDevices(source: string, destination: string): void {
	try {
		renameSync(source, destination);
	} catch (error) {
		if (!(error instanceof Error && 'code' in error && error.code === 'EXDEV'))
			throw error;
		cpSync(source, destination, { recursive: true, preserveTimestamps: true });
		rmSync(source, { recursive: true });
	}
}

function writeAtomically(target: string, content: string): void {
	mkdirSync(path.dirname(target), { recursive: true });
	const temporary = `${target}.migrate-tmp`;
	writeFileSync(temporary, content);
	renameSync(temporary, target);
}

function pruneEmptyDirectories(directory: string, vaultRoot: string): void {
	let current = directory;
	while (
		current !== vaultRoot &&
		current.startsWith(vaultRoot) &&
		existsSync(current) &&
		readdirSync(current).filter((name) => name !== '.DS_Store').length === 0
	) {
		rmSync(current, { recursive: true });
		current = path.dirname(current);
	}
}

function uniquePath(candidate: string): string {
	if (!existsSync(candidate)) return candidate;
	const extension = path.extname(candidate);
	const stem = candidate.slice(0, candidate.length - extension.length);
	for (let suffix = 1; ; suffix++) {
		const next = `${stem} ${suffix}${extension}`;
		if (!existsSync(next)) return next;
	}
}

function sha256(text: string): string {
	return createHash('sha256').update(text, 'utf8').digest('hex');
}
