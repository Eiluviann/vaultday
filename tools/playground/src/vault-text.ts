import { reportNoteText } from '@vaultday/provenance-core';
import {
	addDays,
	type ClockTime,
	type IsoDate,
	isValidIsoDate,
} from './calendar.ts';

/**
 * The moment a generated note must be true at. `doneTimes` holds, per task
 * id, the day and time of the day-log line that finished it, so a task done
 * later today is still open.
 */
export type NoteCutoff = {
	today: IsoDate;
	now: ClockTime;
	doneTimes: ReadonlyMap<string, DoneMoment>;
};

/** When a task's done line was written. */
export type DoneMoment = {
	date: IsoDate;
	time: ClockTime;
};

const ISO_DATE_IN_TEXT_PATTERN = /\b\d{4}-\d{2}-\d{2}\b/g;
const TICKED_TASK_PATTERN =
	/^(\s*- )\[x\]( .*?) \[completion:: (\d{4}-\d{2}-\d{2})\]/;
const TASK_ID_PATTERN = /\[id:: ([^\]]+)\]/;
const CREATED_FIELD_PATTERN = /\[created:: (\d{4}-\d{2}-\d{2})\]/g;
/** A dated list line, as `## Notes` writes them: `- 2026-10-21 Decided …`. */
const DATED_LIST_LINE_PATTERN = /^\s*- (\d{4}-\d{2}-\d{2}) /;
/** A stamped marker, the provenance core's grammar: author, date, hash, then an optional verification; a CRLF note keeps its `\r`. */
const STAMPED_MARKER_PATTERN =
	/^(?<head>(?:> )*%% ai:(?:note|begin) [^\s·]+ · )(?<date>\d{4}-\d{2}-\d{2}) · (?<hash>[0-9a-f]{12})(?<tail>(?: · verified \d{4}-\d{2}-\d{2} · [0-9a-f]{12})? %%\r?)$/;
/** A block's hash covers its body; a note's covers the block markers inside it, so blocks settle first. */
const MAX_PROVENANCE_PASSES = 3;

/**
 * Prepares a fixture note for the playground: its dates move to the focus
 * week, nothing in it happens after the cutoff, and the provenance markers
 * carry the hash of the text they now hold.
 */
export function adaptFixtureNote(
	relativePath: string,
	text: string,
	shiftDays: number,
	cutoff: NoteCutoff,
): string {
	return refreshProvenanceHashes(
		relativePath,
		holdNoteToCutoff(shiftDates(text, shiftDays), cutoff),
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
 * Keeps a note true at the cutoff: a task completed after it is open again,
 * a task created after today was created today, a dated list line after
 * today is not written yet, and a marker dated after today is dated today.
 * Not for `Leave.md`, whose dated lines lie in the future on purpose.
 */
export function holdNoteToCutoff(text: string, cutoff: NoteCutoff): string {
	return text
		.split('\n')
		.filter((line) => !isDatedLineAfter(line, cutoff.today))
		.map((line) =>
			clampMarkerDate(reopenTaskNotDoneYet(line, cutoff), cutoff.today),
		)
		.join('\n')
		.replace(CREATED_FIELD_PATTERN, (field, date: string) =>
			date > cutoff.today ? `[created:: ${cutoff.today}]` : field,
		);
}

/**
 * Rewrites the hash of every `ai:begin` and `ai:note` marker whose text
 * changed, so the generated notes read as model text nobody edited since. A
 * verification token stays as written: it vouches for the text a person read.
 *
 * @throws When a marker still reads as revised after the passes: a marker this
 *   function cannot rewrite, which a fixture change has to fix.
 */
export function refreshProvenanceHashes(
	relativePath: string,
	text: string,
): string {
	let current = text;
	for (let pass = 0; pass < MAX_PROVENANCE_PASSES; pass++) {
		const revised = revisedUnits(relativePath, current);
		if (revised.length === 0) return current;
		const lines = current.split('\n');
		for (const unit of revised) {
			const index = unit.line - 1;
			const markerLine = lines[index];
			if (markerLine !== undefined)
				lines[index] = markerLine.replace(
					STAMPED_MARKER_PATTERN,
					`$<head>$<date> · ${unit.currentHash}$<tail>`,
				);
		}
		current = lines.join('\n');
	}
	const stillRevised = revisedUnits(relativePath, current);
	if (stillRevised.length > 0) {
		throw new Error(
			`${relativePath}: the markers on lines ${stillRevised.map((unit) => unit.line).join(', ')} keep a stale hash`,
		);
	}
	return current;
}

function revisedUnits(relativePath: string, text: string) {
	return reportNoteText(relativePath, text).units.filter(
		(unit) => unit.status === 'revised',
	);
}

function reopenTaskNotDoneYet(line: string, cutoff: NoteCutoff): string {
	const match = TICKED_TASK_PATTERN.exec(line);
	const completion = match?.[3];
	if (match === null || completion === undefined) return line;
	if (completion < cutoff.today) return line;
	if (completion === cutoff.today && !isDoneLaterToday(line, cutoff))
		return line;
	return line.replace(TICKED_TASK_PATTERN, `${match[1]}[ ]${match[2]}`);
}

function isDoneLaterToday(line: string, cutoff: NoteCutoff): boolean {
	const taskId = TASK_ID_PATTERN.exec(line)?.[1];
	const done = taskId === undefined ? undefined : cutoff.doneTimes.get(taskId);
	return (
		done !== undefined && done.date === cutoff.today && done.time > cutoff.now
	);
}

function isDatedLineAfter(line: string, today: IsoDate): boolean {
	const date = DATED_LIST_LINE_PATTERN.exec(line)?.[1];
	return date !== undefined && date > today;
}

function clampMarkerDate(line: string, today: IsoDate): string {
	const date = STAMPED_MARKER_PATTERN.exec(line)?.groups?.date;
	if (date === undefined || date <= today) return line;
	return line.replace(
		STAMPED_MARKER_PATTERN,
		`$<head>${today} · $<hash>$<tail>`,
	);
}
