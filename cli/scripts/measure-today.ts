/**
 * Times `today()` through the native binding over a workspace: 30 calls after
 * 3 warm-ups, printing min / median / p95 in milliseconds.
 *
 * Usage: tsx scripts/measure-today.ts <workspace path> [--date YYYY-MM-DD]
 */
import { openWorkspace, today } from '@vaultday/core-node';

const [, , workspacePath, dateFlag, dateValue] = process.argv;
if (workspacePath === undefined) throw new Error('workspace path is required');
const date = dateFlag === '--date' ? dateValue : undefined;

const workspace = openWorkspace(workspacePath);
for (let warmUp = 0; warmUp < 3; warmUp += 1) today(workspace, date);

const samples: number[] = [];
let lastView = today(workspace, date);
for (let run = 0; run < 30; run += 1) {
	const start = performance.now();
	lastView = today(workspace, date);
	samples.push(performance.now() - start);
}
const sorted = [...samples].sort((a, b) => a - b);
const at = (fraction: number): number =>
	sorted[Math.min(sorted.length - 1, Math.floor(fraction * sorted.length))] ??
	Number.NaN;
console.log(
	`today() over ${lastView.stories.length} stories: min ${at(0).toFixed(2)} ms, median ${at(0.5).toFixed(2)} ms, p95 ${at(0.95).toFixed(2)} ms (n=${samples.length})`,
);
