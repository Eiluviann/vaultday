import { mapOutsideInlineCode } from './markdown-regions.ts';
import {
	currentRegions,
	currentTexts,
	insertLines,
	type NoteDocument,
	replaceLineText,
} from './note-document.ts';
import { isInsideUnit, scanUnits } from './provenance-units.ts';

export type TaskTransformOptions = {
	/** Hands out the next task id; undefined leaves lines without ids alone (templates, agent material). */
	allocateId: (() => string) | undefined;
	/** Story notes get an empty `## Next Steps` above `## Pending Input`. */
	shouldInsertNextSteps: boolean;
};

export type TaskTransformOutcome = { idsInserted: string[]; reports: string[] };

const TASK_LINE_PATTERN = /^((?:> )*\s*)([-*+]|\d+[.)]) \[(.)\] (.*)$/;
const ALLOWED_STATUS_CHARACTERS = new Set([' ', 'x', '-', '/']);
const ID_FIELD_PATTERN = /\[id::\s*[^\]]+\]/;
const FIRST_FIELD_PATTERN = /\s*\[[a-z_]+::/;
const NEXT_STEPS_HEADING = '## Next Steps';
const PENDING_INPUT_HEADING = '## Pending Input';

/**
 * Task rule table: `[X]` → `[x]`, `- [1]` → plain bullet, `[k::v]` → `[k:: v]`,
 * `[id:: task-N]` appended once, `## Next Steps` inserted before `## Pending Input`
 * in stories. Never touches fenced code, frontmatter or marker lines and never
 * invents completion dates.
 */
export function transformTaskLines(
	document: NoteDocument,
	options: TaskTransformOptions,
): TaskTransformOutcome {
	const outcome: TaskTransformOutcome = { idsInserted: [], reports: [] };
	const regions = currentRegions(document);
	const texts = currentTexts(document);
	texts.forEach((text, index) => {
		if (regions[index]?.region !== 'prose') return;
		const match = TASK_LINE_PATTERN.exec(text);
		if (match === null) return;
		const [, indent = '', bullet = '-', status = ' ', rest = ''] = match;
		if (status === '1') {
			replaceLineText(document, index, `${indent}${bullet} ${rest}`, 'task');
			return;
		}
		const normalisedStatus = status === 'X' ? 'x' : status;
		if (!ALLOWED_STATUS_CHARACTERS.has(normalisedStatus)) {
			outcome.reports.push(
				`${document.relativePath}:${index + 1}: unknown task status [${status}] left unchanged`,
			);
			return;
		}
		let newRest = mapOutsideInlineCode(rest, (segment) =>
			segment.replace(/\[([a-z_]+)::(?=\S)/g, '[$1:: '),
		);
		if (options.allocateId !== undefined && !ID_FIELD_PATTERN.test(newRest)) {
			const id = options.allocateId();
			outcome.idsInserted.push(id);
			newRest = insertIdField(newRest, id);
		}
		replaceLineText(
			document,
			index,
			`${indent}${bullet} [${normalisedStatus}] ${newRest}`,
			'task',
		);
	});
	if (options.shouldInsertNextSteps) insertNextSteps(document, outcome);
	return outcome;
}

function insertIdField(rest: string, id: string): string {
	const fieldMatch = FIRST_FIELD_PATTERN.exec(rest);
	if (fieldMatch === null) return `${rest.trimEnd()} [id:: ${id}]`;
	const fieldStart = fieldMatch.index + fieldMatch[0].indexOf('[');
	return `${rest.slice(0, fieldStart)}[id:: ${id}] ${rest.slice(fieldStart)}`;
}

function insertNextSteps(
	document: NoteDocument,
	outcome: TaskTransformOutcome,
): void {
	const texts = currentTexts(document);
	const regions = currentRegions(document);
	const hasNextSteps = texts.some(
		(text, index) =>
			regions[index]?.region === 'prose' && text.trim() === NEXT_STEPS_HEADING,
	);
	if (hasNextSteps) return;
	const pendingIndex = texts.findIndex(
		(text, index) =>
			regions[index]?.region === 'prose' &&
			text.trim() === PENDING_INPUT_HEADING,
	);
	if (pendingIndex === -1) return;
	if (isInsideUnit(scanUnits(texts), pendingIndex)) {
		outcome.reports.push(
			`${document.relativePath}:${pendingIndex + 1}: ## Next Steps not inserted (## Pending Input sits inside an AI unit; add it by hand)`,
		);
		return;
	}
	insertLines(document, pendingIndex, [NEXT_STEPS_HEADING], 'structure');
}
