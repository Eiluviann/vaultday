import { existsSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { applyPlan } from './apply.ts';
import { formatLocalDate, readBirthtimeManifest } from './birthtimes.ts';
import {
	type CliOptions,
	guardVaultPath,
	parseCliOptions,
	USAGE,
} from './cli-options.ts';
import { IdAllocator } from './id-allocator.ts';
import { buildPlan, isPlanEmpty, type MigrationPlan } from './migration.ts';
import {
	emptyProfile,
	loadMoves,
	loadProfile,
	type MoveSpec,
	type Profile,
} from './profile.ts';
import {
	loadProvenanceCore,
	type ProvenanceHasher,
} from './provenance-core.ts';
import { renderPlan } from './render-plan.ts';
import { err, ok, type Result } from './result.ts';
import { buildTickPlan } from './tick.ts';
import { verifyLinks } from './verify-links.ts';

const PACKAGE_ROOT = path.resolve(
	path.dirname(fileURLToPath(import.meta.url)),
	'..',
);

process.exitCode = await main(process.argv.slice(2));

async function main(argv: string[]): Promise<number> {
	const optionsResult = parseCliOptions(argv, PACKAGE_ROOT);
	if (!optionsResult.ok) {
		console.error(`${optionsResult.error}\n\n${USAGE}`);
		return 2;
	}
	const options = optionsResult.value;
	const guard = guardVaultPath(options);
	if (!guard.ok) {
		console.error(guard.error);
		return 2;
	}
	if (options.shouldVerifyLinks) return runVerifyLinks(options.vaultRoot);

	const coreResult = await loadProvenanceCore(options.coreModulePath);
	if (!coreResult.ok) {
		console.error(coreResult.error);
		return 2;
	}
	const allocator = new IdAllocator(
		options.vaultRoot,
		options.idStateDirectory,
	);
	const planResult =
		options.tickId === undefined
			? buildMigrationPlan(options, allocator, coreResult.value)
			: buildTickPlan(
					options.vaultRoot,
					options.tickId,
					formatLocalDate(new Date()),
				);
	if (!planResult.ok) {
		console.error(planResult.error);
		return 2;
	}
	const plan = planResult.value;
	if (isPlanEmpty(plan)) return 0;
	process.stdout.write(renderPlan(plan));
	if (options.mode === 'dry-run') return 0;

	const applied = applyPlan(plan, {
		stateDirectory: options.stateDirectory,
		allocator,
	});
	if (!applied.ok) {
		console.error(`apply aborted before writing: ${applied.error}`);
		return 1;
	}
	console.log(
		`\napplied: ${applied.value.files.length} file(s) written, ${applied.value.renames.length} rename(s), ${applied.value.moves.length} move(s), ${applied.value.deletes.length} delete(s); manifest in ${options.stateDirectory}`,
	);
	if (options.tickId !== undefined) return 0;
	return checkIdempotence(options, coreResult.value);
}

function runVerifyLinks(vaultRoot: string): number {
	const unresolved = verifyLinks(vaultRoot);
	for (const link of unresolved)
		console.log(`${link.relativePath}:${link.line}  [[${link.target}]]`);
	console.log(`${unresolved.length} unresolved link(s)`);
	return unresolved.length === 0 ? 0 : 1;
}

function buildMigrationPlan(
	options: CliOptions,
	allocator: IdAllocator,
	hasher: ProvenanceHasher,
): Result<MigrationPlan> {
	const configResult = loadConfig(options);
	if (!configResult.ok) return configResult;
	const birthtimes =
		options.birthtimesPath === undefined
			? new Map<string, string>()
			: readBirthtimeManifest(options.birthtimesPath);
	return buildPlan({
		vaultRoot: options.vaultRoot,
		...configResult.value,
		rules: options.rules,
		birthtimes,
		allocator,
		hasher,
	});
}

function loadConfig(
	options: CliOptions,
): Result<{ profile: Profile; moveSpecs: MoveSpec[] }> {
	const profileName =
		options.profileName ?? path.basename(options.vaultRoot).toLowerCase();
	const profilePath = path.join(options.configDirectory, `${profileName}.json`);
	const movesPath = path.join(
		options.configDirectory,
		`${profileName}.moves.json`,
	);
	if (!existsSync(profilePath)) {
		if (options.profileName !== undefined)
			return err(`profile not found: ${profilePath}`);
		console.error(
			`no profile at ${profilePath}; running with an empty "${profileName.includes('personal') ? 'personal' : 'work'}" profile`,
		);
		return ok({
			profile: emptyProfile(
				profileName.includes('personal') ? 'personal' : 'work',
			),
			moveSpecs: [],
		});
	}
	const profile = loadProfile(profilePath);
	if (!profile.ok) return profile;
	const moves = existsSync(movesPath) ? loadMoves(movesPath) : ok([]);
	if (!moves.ok) return moves;
	return ok({ profile: profile.value, moveSpecs: moves.value });
}

function checkIdempotence(
	options: CliOptions,
	hasher: ProvenanceHasher,
): number {
	const secondPass = buildMigrationPlan(
		options,
		new IdAllocator(options.vaultRoot, options.idStateDirectory),
		hasher,
	);
	if (!secondPass.ok) {
		console.error(`idempotence check failed to build: ${secondPass.error}`);
		return 1;
	}
	if (isPlanEmpty(secondPass.value)) {
		console.log('second pass: no changes (idempotent)');
		return 0;
	}
	console.error('second pass still finds changes:');
	process.stderr.write(renderPlan(secondPass.value));
	return 1;
}
