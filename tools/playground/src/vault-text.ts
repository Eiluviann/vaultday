import { reportNoteText } from '@vaultday/provenance-core';
import { addDays, type IsoDate, isValidIsoDate } from './calendar.ts';

const ISO_DATE_IN_TEXT_PATTERN = /\b\d{4}-\d{2}-\d{2}\b/g;
const TICKED_TASK_PATTERN =
	/^(\s*- )\[x\]( .*?) \[completion:: (\d{4}-\d{2}-\d{2})\]/;
const CREATED_FIELD_PATTERN = /\[created:: (\d{4}-\d{2}-\d{2})\]/g;
const MARKER_HASH_PATTERN = /( · )([0-9a-f]{12})( %%| verified)/;
/** A block's hash covers its body; a note's covers the block markers inside it, so blocks settle first. */
const MAX_PROVENANCE_PASSES = 3;

/**
 * Prepares a fixture note for the playground: its dates move to the focus
 * week, nothing is done or created after today, and the provenance markers
 * carry the hash of the text they now hold.
 */
export function adaptFixtureNote(
	relativePath: string,
	text: string,
	shiftDays: number,
	today: IsoDate,
): string {
	return refreshProvenanceHashes(
		relativePath,
		settleTasksAfter(shiftDates(text, shiftDays), today),
	);
}

/** Moves every ISO date in `text` by `days`; strings that only look like dates stay. */
export function shiftDates(text: string, days: number): string {
	if (days === 0) return text;
	return text.replace(ISO_DATE_IN_TEXT_PATTERN, (date) =>
		isValidIsoDate(date) ? addDays(date, days) : date,
	);
}

/**
 * Keeps a note true on `today`: a task completed after it is open again, and
 * a task created after it was created today.
 */
export function settleTasksAfter(text: string, today: IsoDate): string {
	return text
		.split('\n')
		.map((line) => reopenTaskCompletedAfter(line, today))
		.join('\n')
		.replace(CREATED_FIELD_PATTERN, (field, date: string) =>
			date > today ? `[created:: ${today}]` : field,
		);
}

/**
 * Rewrites the hash of every `ai:begin` and `ai:note` marker whose text
 * changed, so the generated notes read as model text nobody edited since.
 */
export function refreshProvenanceHashes(
	relativePath: string,
	text: string,
): string {
	let current = text;
	for (let pass = 0; pass < MAX_PROVENANCE_PASSES; pass++) {
		const revised = reportNoteText(relativePath, current).units.filter(
			(unit) => unit.status === 'revised',
		);
		if (revised.length === 0) return current;
		const lines = current.split('\n');
		for (const unit of revised) {
			const index = unit.line - 1;
			const markerLine = lines[index];
			if (markerLine !== undefined)
				lines[index] = markerLine.replace(
					MARKER_HASH_PATTERN,
					`$1${unit.currentHash}$3`,
				);
		}
		current = lines.join('\n');
	}
	return current;
}

function reopenTaskCompletedAfter(line: string, today: IsoDate): string {
	const match = TICKED_TASK_PATTERN.exec(line);
	const completion = match?.[3];
	if (match === null || completion === undefined || completion <= today)
		return line;
	return line.replace(TICKED_TASK_PATTERN, `${match[1]}[ ]${match[2]}`);
}
