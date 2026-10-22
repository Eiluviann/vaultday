import { unifiedDiff } from './diff.ts';
import type { MigrationPlan } from './migration.ts';
import { isNotePath } from './vault-scan.ts';

/** Dry-run output: unified diffs, renames, moves, deletes, then a summary block. Empty string for an empty plan. */
export function renderPlan(plan: MigrationPlan): string {
	const sections: string[] = [];
	for (const change of plan.changes) {
		const destination =
			change.externalPath ??
			(change.toPath === undefined ? '(removed)' : `b/${change.toPath}`);
		if (change.before !== change.after)
			sections.push(
				unifiedDiff(
					change.before,
					change.after,
					`a/${change.fromPath}`,
					destination,
				),
			);
		else sections.push(`rename only: ${change.fromPath} -> ${destination}`);
	}
	for (const rename of plan.renames)
		sections.push(
			`rename folder: ${rename.fromDirectory} -> ${rename.toDirectory}`,
		);
	for (const move of plan.moves)
		sections.push(
			`move ${move.kind}: ${move.from} -> ${move.externalPath ?? move.to ?? '(removed)'}`,
		);
	for (const entry of plan.deletes)
		sections.push(`delete ${entry.kind}: ${entry.path} -> .trash/`);
	sections.push(renderSummary(plan));
	return `${sections.join('\n\n')}\n`;
}

export function renderSummary(plan: MigrationPlan): string {
	const lines: string[] = ['# Summary', `vault: ${plan.vaultRoot}`];
	const notesTouched = plan.changes.filter(
		(change) => change.before !== change.after && isNotePath(change.fromPath),
	).length;
	lines.push(
		`notes with content changes: ${notesTouched}`,
		`folder renames: ${plan.renames.length}`,
		`moves: ${plan.moves.length}`,
		`deletes: ${plan.deletes.length}`,
	);
	if (plan.allocatedIds.length > 0)
		lines.push(
			`task ids allocated: ${plan.allocatedIds.length} (${plan.allocatedIds[0]} … ${plan.allocatedIds.at(-1)})`,
		);
	if (plan.renames.length > 0) {
		lines.push(
			'',
			'## Renames',
			'| Number | Old title | New folder |',
			'| --- | --- | --- |',
		);
		for (const rename of plan.renames)
			lines.push(
				`| ${rename.number} | ${rename.oldTitle} | ${rename.newTitle} |`,
			);
	}
	if (plan.hubs.length > 0) {
		lines.push('', '## Project hubs');
		for (const hub of plan.hubs) lines.push(`- ${hub.path}: key ${hub.key}`);
	}
	if (plan.typeGuesses.length > 0) {
		lines.push('', '## Type guesses');
		for (const guess of plan.typeGuesses)
			lines.push(
				`- ${guess.path}: type ${guess.type}${guess.kind === undefined ? '' : `, kind ${guess.kind}`}`,
			);
	}
	const refreshes = plan.changes.flatMap((change) =>
		change.refreshes.map((refresh) => ({ path: change.fromPath, refresh })),
	);
	const refreshed = refreshes.filter(
		({ refresh }) => refresh.result === 'refreshed',
	);
	const revised = refreshes.filter(
		({ refresh }) => refresh.result !== 'refreshed',
	);
	if (refreshed.length > 0) {
		lines.push('', `## Units re-fingerprinted (${refreshed.length})`);
		for (const { path, refresh } of refreshed)
			lines.push(
				`- ${path}:${refresh.markerLine} ${refresh.kind} ${refresh.oldHash} -> ${refresh.newHash} (${refresh.edits.join(', ')})`,
			);
	}
	if (revised.length > 0) {
		lines.push('', `## Units left as revised or bare (${revised.length})`);
		for (const { path, refresh } of revised)
			lines.push(
				`- ${path}:${refresh.markerLine} ${refresh.kind} ${refresh.result} (${refresh.edits.join(', ')})`,
			);
	}
	if (plan.reports.length > 0) {
		lines.push('', '## Reports');
		for (const report of plan.reports) lines.push(`- ${report}`);
	}
	return lines.join('\n');
}
