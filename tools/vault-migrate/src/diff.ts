const CONTEXT_LINES = 3;

type DiffOperation = { type: 'equal' | 'insert' | 'delete'; text: string };

/** Unified diff of two texts (3 lines of context), '' when identical. */
export function unifiedDiff(
	oldText: string,
	newText: string,
	oldLabel: string,
	newLabel: string,
): string {
	if (oldText === newText) return '';
	const operations = diffLines(oldText.split('\n'), newText.split('\n'));
	const hunks = buildHunks(operations);
	return [`--- ${oldLabel}`, `+++ ${newLabel}`, ...hunks].join('\n');
}

function diffLines(oldLines: string[], newLines: string[]): DiffOperation[] {
	const rows = oldLines.length;
	const columns = newLines.length;
	const table = new Int32Array((rows + 1) * (columns + 1));
	const at = (row: number, column: number) => row * (columns + 1) + column;
	for (let row = rows - 1; row >= 0; row--) {
		for (let column = columns - 1; column >= 0; column--) {
			table[at(row, column)] =
				oldLines[row] === newLines[column]
					? (table[at(row + 1, column + 1)] ?? 0) + 1
					: Math.max(
							table[at(row + 1, column)] ?? 0,
							table[at(row, column + 1)] ?? 0,
						);
		}
	}
	const operations: DiffOperation[] = [];
	let row = 0;
	let column = 0;
	while (row < rows && column < columns) {
		if (oldLines[row] === newLines[column]) {
			operations.push({ type: 'equal', text: oldLines[row] ?? '' });
			row++;
			column++;
		} else if (
			(table[at(row + 1, column)] ?? 0) >= (table[at(row, column + 1)] ?? 0)
		) {
			operations.push({ type: 'delete', text: oldLines[row] ?? '' });
			row++;
		} else {
			operations.push({ type: 'insert', text: newLines[column] ?? '' });
			column++;
		}
	}
	while (row < rows)
		operations.push({ type: 'delete', text: oldLines[row++] ?? '' });
	while (column < columns)
		operations.push({ type: 'insert', text: newLines[column++] ?? '' });
	return operations;
}

function buildHunks(operations: DiffOperation[]): string[] {
	const changedIndexes = operations.flatMap((operation, index) =>
		operation.type === 'equal' ? [] : [index],
	);
	const output: string[] = [];
	let hunkStart = -1;
	let hunkEnd = -1;
	const flush = () => {
		if (hunkStart === -1) return;
		output.push(...renderHunk(operations, hunkStart, hunkEnd));
		hunkStart = -1;
	};
	for (const index of changedIndexes) {
		const start = Math.max(0, index - CONTEXT_LINES);
		const end = Math.min(operations.length - 1, index + CONTEXT_LINES);
		if (hunkStart !== -1 && start <= hunkEnd + 1) {
			hunkEnd = end;
			continue;
		}
		flush();
		hunkStart = start;
		hunkEnd = end;
	}
	flush();
	return output;
}

function renderHunk(
	operations: DiffOperation[],
	start: number,
	end: number,
): string[] {
	let oldLine = 1;
	let newLine = 1;
	for (let index = 0; index < start; index++) {
		const operation = operations[index];
		if (operation?.type !== 'insert') oldLine++;
		if (operation?.type !== 'delete') newLine++;
	}
	const slice = operations.slice(start, end + 1);
	const oldCount = slice.filter(
		(operation) => operation.type !== 'insert',
	).length;
	const newCount = slice.filter(
		(operation) => operation.type !== 'delete',
	).length;
	const prefix = { equal: ' ', insert: '+', delete: '-' } as const;
	return [
		`@@ -${oldLine},${oldCount} +${newLine},${newCount} @@`,
		...slice.map((operation) => `${prefix[operation.type]}${operation.text}`),
	];
}
