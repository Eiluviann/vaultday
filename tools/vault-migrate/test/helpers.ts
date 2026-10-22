import { cpSync, mkdirSync, mkdtempSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { readBirthtimeManifest } from '../src/birthtimes.ts';
import { IdAllocator } from '../src/id-allocator.ts';
import {
	ALL_RULES,
	type BuildPlanOptions,
	buildPlan,
	type MigrationPlan,
	type RuleName,
} from '../src/migration.ts';
import { loadMoves, loadProfile } from '../src/profile.ts';
import {
	loadProvenanceCore,
	type ProvenanceHasher,
} from '../src/provenance-core.ts';

export const PACKAGE_ROOT = path.resolve(
	path.dirname(fileURLToPath(import.meta.url)),
	'..',
);
export const FIXTURES_ROOT = path.join(PACKAGE_ROOT, 'fixtures');

export type FixtureWorkspace = {
	root: string;
	work: string;
	personal: string;
	idStateDirectory: string;
	stateDirectory: string;
};

/** Copies both mini vaults into a fresh temp folder (never the fixtures themselves). */
export function copyFixtures(): FixtureWorkspace {
	const root = mkdtempSync(path.join(tmpdir(), 'vault-migrate-'));
	cpSync(path.join(FIXTURES_ROOT, 'mini-work'), path.join(root, 'mini-work'), {
		recursive: true,
	});
	cpSync(
		path.join(FIXTURES_ROOT, 'mini-personal'),
		path.join(root, 'mini-personal'),
		{ recursive: true },
	);
	mkdirSync(path.join(root, 'mini-personal', 'Daily'));
	return {
		root,
		work: path.join(root, 'mini-work'),
		personal: path.join(root, 'mini-personal'),
		idStateDirectory: path.join(root, 'ids'),
		stateDirectory: path.join(root, 'state'),
	};
}

export async function loadHasher(): Promise<ProvenanceHasher> {
	const core = await loadProvenanceCore();
	if (!core.ok) throw new Error(core.error);
	return core.value;
}

export type PlanOverrides = Partial<
	Pick<BuildPlanOptions, 'rules' | 'birthtimes'>
>;

export function planFixture(
	vaultRoot: string,
	profileName: 'mini-work' | 'mini-personal',
	workspace: FixtureWorkspace,
	hasher: ProvenanceHasher,
	overrides: PlanOverrides = {},
): { plan: MigrationPlan; allocator: IdAllocator } {
	const profile = loadProfile(path.join(FIXTURES_ROOT, `${profileName}.json`));
	const moves = loadMoves(
		path.join(FIXTURES_ROOT, `${profileName}.moves.json`),
	);
	if (!profile.ok) throw new Error(profile.error);
	if (!moves.ok) throw new Error(moves.error);
	const allocator = new IdAllocator(vaultRoot, workspace.idStateDirectory);
	const birthtimes =
		overrides.birthtimes ??
		(profileName === 'mini-work'
			? readBirthtimeManifest(
					path.join(FIXTURES_ROOT, 'mini-work.birthtimes.tsv'),
				)
			: new Map<string, string>());
	const result = buildPlan({
		vaultRoot,
		profile: profile.value,
		moveSpecs: moves.value,
		rules: overrides.rules ?? new Set<RuleName>(ALL_RULES),
		birthtimes,
		allocator,
		hasher,
	});
	if (!result.ok) throw new Error(result.error);
	return { plan: result.value, allocator };
}

export function changeFor(
	plan: MigrationPlan,
	fromPath: string,
): { before: string; after: string } {
	const change = plan.changes.find(
		(candidate) => candidate.fromPath === fromPath,
	);
	if (change === undefined)
		throw new Error(
			`no change planned for ${fromPath}; planned: ${plan.changes.map((candidate) => candidate.fromPath).join(', ')}`,
		);
	return change;
}

export function frontmatterOf(text: string): string[] {
	const lines = text.split('\n');
	const end = lines.indexOf('---', 1);
	return lines.slice(1, end);
}

export function readVaultFile(vaultRoot: string, relativePath: string): string {
	return readFileSync(path.join(vaultRoot, relativePath), 'utf8');
}

export const WIDGET_STORY =
	'Projects/Alpha/Widget colour picker (new look)/Widget colour picker (new look).md';
export const LONG_STORY =
	'Projects/Alpha/A very long story title that certainly exceeds forty characters (details)/A very long story title that certainly exceeds forty characters (details).md';
export const UNNUMBERED_STORY =
	'Projects/Alpha/Unnumbered draft/Unnumbered draft.md';
export const HUB = 'Projects/Alpha/Alpha.md';
export const CURRENT_WIDGET =
	'Projects/Alpha/Widget colour picker (new look)/Current widget.md';
