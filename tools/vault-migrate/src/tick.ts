import { classifyLines } from './markdown-regions.ts';
import type { MigrationPlan } from './migration.ts';
import { err, ok, type Result } from './result.ts';
import { isNotePath, readNoteText, scanVault } from './vault-scan.ts';

const TASK_LINE_PATTERN = /^((?:> )*\s*(?:[-*+]|\d+[.)]) )\[(.)\]( .*)$/;

/**
 * Toggles the one task line carrying `[id:: <id>]`: open → done with a
 * completion date, done → open without it. Exactly one line changes.
 */
export function buildTickPlan(
	vaultRoot: string,
	id: string,
	today: string,
): Result<MigrationPlan> {
	const hits: Array<{
		relativePath: string;
		text: string;
		lines: string[];
		index: number;
	}> = [];
	const idField = `[id:: ${id}]`;
	for (const entry of scanVault(vaultRoot).filter(
		(candidate) =>
			candidate.kind === 'file' && isNotePath(candidate.relativePath),
	)) {
		const text = readNoteText(entry.absolutePath);
		if (!text.includes(idField)) continue;
		const lines = text.split('\n');
		const regions = classifyLines(lines);
		lines.forEach((line, index) => {
			if (
				regions[index]?.region === 'prose' &&
				line.includes(idField) &&
				TASK_LINE_PATTERN.test(line)
			)
				hits.push({ relativePath: entry.relativePath, text, lines, index });
		});
	}
	const hit = hits[0];
	if (hit === undefined) return err(`no task line carries ${idField}`);
	if (hits.length > 1)
		return err(
			`${idField} appears on ${hits.length} task lines; ids must be unique`,
		);
	const match = TASK_LINE_PATTERN.exec(hit.lines[hit.index] ?? '');
	if (match === null) return err('task line did not parse');
	const [, head = '', status = ' ', rest = ''] = match;
	const toggled =
		status === 'x'
			? `${head}[ ]${rest.replace(/\s*\[completion::\s*[^\]]*\]/, '')}`
			: `${head}[x]${rest.replace(/\s*\[completion::\s*[^\]]*\]/, '')} [completion:: ${today}]`;
	const lines = [...hit.lines];
	lines[hit.index] = toggled;
	const after = lines.join('\n');
	return ok({
		vaultRoot,
		changes: [
			{
				fromPath: hit.relativePath,
				toPath: hit.relativePath,
				before: hit.text,
				after,
				edits: new Map([['task', 1]]),
				refreshes: [],
				noteClass: 'other',
			},
		],
		renames: [],
		moves: [],
		deletes: [],
		allocatedIds: [],
		hubs: [],
		typeGuesses: [],
		reports: [],
	});
}
