import { existsSync } from 'node:fs';
import path from 'node:path';
import { ALL_RULES, type RuleName } from './migration.ts';
import { err, ok, type Result } from './result.ts';

export type CliCommand = 'migrate' | 'lint-log';

export type CliOptions = {
	command: CliCommand;
	vaultRoot: string;
	mode: 'dry-run' | 'apply';
	rules: Set<RuleName>;
	profileName: string | undefined;
	configDirectory: string;
	stateDirectory: string;
	idStateDirectory: string | undefined;
	birthtimesPath: string | undefined;
	coreModulePath: string | undefined;
	shouldVerifyLinks: boolean;
	tickId: string | undefined;
	isLiveAllowed: boolean;
};

export const USAGE = [
	'vault-migrate --vault <path> (--dry-run | --apply) [options]',
	'vault-migrate lint-log --vault <path>            validate Log/*.md against the day-log grammar',
	'',
	'  --rules frontmatter,tasks,rename,queries,moves   rules to run (default: all)',
	'  --profile <name>        config/<name>.json and config/<name>.moves.json (default: vault folder name, lowercased)',
	'  --config-dir <dir>      where the profile files live (default: <package>/config)',
	'  --birthtimes <tsv>      stat -f "%B%t%N" manifest of the live vault, source of `created`',
	'  --state-dir <dir>       where apply manifests go (default: <package>/state)',
	'  --id-state-dir <dir>    where task-id counters live (default: ~/.local/state/vaultday/ids)',
	'  --core <path>           provenance core module (default: ~/.claude/hooks/lib/obsidian-ai-provenance-core.mjs)',
	'  --verify-links          list unresolved wikilinks and exit',
	'  --tick <id>             toggle the task line carrying [id:: <id>] (with --dry-run or --apply)',
	'  --allow-live            permit --apply outside ~/Vaults/Preview',
].join('\n');

export function parseCliOptions(
	argv: string[],
	packageRoot: string,
): Result<CliOptions> {
	const values = new Map<string, string>();
	const flags = new Set<string>();
	const command: CliCommand = argv[0] === 'lint-log' ? 'lint-log' : 'migrate';
	for (
		let index = command === 'lint-log' ? 1 : 0;
		index < argv.length;
		index++
	) {
		const argument = argv[index] ?? '';
		if (!argument.startsWith('--'))
			return err(`unexpected argument: ${argument}`);
		const name = argument.slice(2);
		if (['dry-run', 'apply', 'verify-links', 'allow-live'].includes(name)) {
			flags.add(name);
			continue;
		}
		const value = argv[index + 1];
		if (value === undefined || value.startsWith('--'))
			return err(`--${name} needs a value`);
		values.set(name, value);
		index++;
	}
	const vault = values.get('vault');
	if (vault === undefined) return err('--vault is required');
	const hasDryRun = flags.has('dry-run');
	const hasApply = flags.has('apply');
	if (
		command === 'migrate' &&
		!flags.has('verify-links') &&
		hasDryRun === hasApply
	)
		return err('pass exactly one of --dry-run or --apply');
	const rulesResult = parseRules(values.get('rules'));
	if (!rulesResult.ok) return rulesResult;
	return ok({
		command,
		vaultRoot: path.resolve(vault),
		mode: hasApply ? 'apply' : 'dry-run',
		rules: rulesResult.value,
		profileName: values.get('profile'),
		configDirectory:
			values.get('config-dir') ?? path.join(packageRoot, 'config'),
		stateDirectory: values.get('state-dir') ?? path.join(packageRoot, 'state'),
		idStateDirectory: values.get('id-state-dir'),
		birthtimesPath: values.get('birthtimes'),
		coreModulePath: values.get('core'),
		shouldVerifyLinks: flags.has('verify-links'),
		tickId: values.get('tick'),
		isLiveAllowed: flags.has('allow-live'),
	});
}

function parseRules(raw: string | undefined): Result<Set<RuleName>> {
	if (raw === undefined) return ok(new Set(ALL_RULES));
	const rules = new Set<RuleName>();
	for (const token of raw.split(',').map((part) => part.trim())) {
		const rule = ALL_RULES.find((candidate) => candidate === token);
		if (rule === undefined)
			return err(`unknown rule: ${token} (known: ${ALL_RULES.join(', ')})`);
		rules.add(rule);
	}
	return ok(rules);
}

/** Refuses iCloud copies, non-vaults and live vaults (unless --allow-live). */
export function guardVaultPath(options: CliOptions): Result<undefined> {
	if (options.vaultRoot.includes('Mobile Documents'))
		return err(
			'refusing to touch an iCloud copy (path contains "Mobile Documents")',
		);
	if (!existsSync(path.join(options.vaultRoot, '.obsidian')))
		return err(
			`${options.vaultRoot} is not an Obsidian vault (no .obsidian folder)`,
		);
	if (
		options.mode === 'apply' &&
		!options.isLiveAllowed &&
		!options.vaultRoot.split(path.sep).includes('Preview')
	) {
		return err(
			'refusing to --apply outside a Preview vault; pass --allow-live to override',
		);
	}
	return ok(undefined);
}
