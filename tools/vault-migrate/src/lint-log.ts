import { splitFrontmatterBlock } from './frontmatter.ts';
import { classifyLines } from './markdown-regions.ts';
import { basenameWithoutExtension } from './paths.ts';
import {
	isNotePath,
	LOG_DIRECTORY,
	readNoteText,
	scanVault,
} from './vault-scan.ts';

export type LogFinding = {
	relativePath: string;
	line: number;
	rule: string;
	message: string;
};

export const LOG_KINDS = [
	'session',
	'done',
	'blocker',
	'decision',
	'status',
	'note',
] as const;

/** Story numbers, project hub names and task ids the log lines may point at. */
type VaultIndex = {
	storyNumbers: Set<string>;
	projectNames: Set<string>;
	taskIds: Set<string>;
};

type LogItem = { line: number; text: string };

const LOG_FILE_NAME_PATTERN = /^(\d{4})-(\d{2})-(\d{2})\.md$/;
const LOG_HEADING = '## Log';
const TIME_PREFIX_PATTERN = /^- (?:[01]\d|2[0-3]):[0-5]\d /;
const INLINE_FIELD_PATTERN = /\[([a-z_]+)::([^\]]*)\]/g;
const UNSPACED_FIELD_PATTERN = /\[[a-z_]+::(?!\s)/;
const STORY_NUMBER_PATTERN = /^STRY\d{7}$/;
const TASK_ID_PATTERN = /^task-\d+$/;

/**
 * Validates every `Log/*.md` note against the day-log grammar: dated file name
 * and frontmatter, a `## Log` section of `HH:MM` list items with exactly one
 * `kind`, at most one story or project reference that exists, an existing task
 * id when given, an agent token without whitespace, no nesting and no pipes.
 */
export function lintLogs(vaultRoot: string): LogFinding[] {
	const entries = scanVault(vaultRoot).filter(
		(entry) => entry.kind === 'file' && isNotePath(entry.relativePath),
	);
	const index = buildVaultIndex(
		entries.map((entry) => ({
			relativePath: entry.relativePath,
			text: readNoteText(entry.absolutePath),
		})),
	);
	return entries
		.filter((entry) => entry.relativePath.startsWith(`${LOG_DIRECTORY}/`))
		.flatMap((entry) =>
			lintLogNote(entry.relativePath, readNoteText(entry.absolutePath), index),
		);
}

export function lintLogNote(
	relativePath: string,
	text: string,
	index: VaultIndex,
): LogFinding[] {
	const findings: LogFinding[] = [];
	const report = (line: number, rule: string, message: string) =>
		findings.push({ relativePath, line, rule, message });
	const fileName = relativePath.split('/').at(-1) ?? '';
	const dateFromName = logDateFromFileName(fileName);
	if (dateFromName === undefined) {
		report(
			1,
			'file-name',
			`log file name must be YYYY-MM-DD.md, got ${fileName}`,
		);
		return findings;
	}
	const lines = text.split('\n');
	const { blockEnd, frontmatter } = splitFrontmatterBlock(lines);
	if (frontmatter.scalar('type') !== 'log')
		report(1, 'frontmatter-type', 'frontmatter must carry type: log');
	if (frontmatter.scalar('date') !== dateFromName)
		report(
			1,
			'frontmatter-date',
			`frontmatter date must equal the file name date ${dateFromName}`,
		);
	const section = logSection(lines, blockEnd);
	if (section === undefined) {
		report(blockEnd + 1, 'log-section', `no ${LOG_HEADING} section`);
		return findings;
	}
	for (const item of section.nestedLines)
		report(
			item.line,
			'nested-bullet',
			'nested bullets are not allowed under a log line',
		);
	for (const item of section.items) lintLogItem(item, index, report);
	return findings;
}

export function buildVaultIndex(
	notes: Array<{ relativePath: string; text: string }>,
): VaultIndex {
	const index: VaultIndex = {
		storyNumbers: new Set(),
		projectNames: new Set(),
		taskIds: new Set(),
	};
	for (const note of notes) {
		const { frontmatter } = splitFrontmatterBlock(note.text.split('\n'));
		const number =
			frontmatter.scalar('number') ?? frontmatter.scalar('Number') ?? '';
		if (number.length > 0) index.storyNumbers.add(number);
		if (frontmatter.scalar('type') === 'project')
			index.projectNames.add(basenameWithoutExtension(note.relativePath));
		for (const match of note.text.matchAll(/\[id::\s*(task-\d+)\]/g)) {
			const id = match[1];
			if (id !== undefined) index.taskIds.add(id);
		}
	}
	return index;
}

function logDateFromFileName(fileName: string): string | undefined {
	const match = LOG_FILE_NAME_PATTERN.exec(fileName);
	if (match === null) return undefined;
	const [, year = '', month = '', day = ''] = match;
	const date = new Date(Date.UTC(Number(year), Number(month) - 1, Number(day)));
	const isRealDate =
		date.getUTCFullYear() === Number(year) &&
		date.getUTCMonth() === Number(month) - 1 &&
		date.getUTCDate() === Number(day);
	return isRealDate ? `${year}-${month}-${day}` : undefined;
}

/** Top-level list items of the `## Log` section and any indented list lines beneath them. */
function logSection(
	lines: string[],
	bodyStart: number,
): { items: LogItem[]; nestedLines: LogItem[] } | undefined {
	const regions = classifyLines(lines);
	const headingIndex = lines.findIndex(
		(line, index) =>
			index >= bodyStart &&
			regions[index]?.region === 'prose' &&
			line.trim() === LOG_HEADING,
	);
	if (headingIndex === -1) return undefined;
	const items: LogItem[] = [];
	const nestedLines: LogItem[] = [];
	for (let index = headingIndex + 1; index < lines.length; index++) {
		const line = lines[index] ?? '';
		if (regions[index]?.region !== 'prose') continue;
		if (/^#{1,6} /.test(line)) break;
		if (/^- /.test(line)) items.push({ line: index + 1, text: line });
		else if (/^\s+[-*+] /.test(line))
			nestedLines.push({ line: index + 1, text: line });
	}
	return { items, nestedLines };
}

function lintLogItem(
	item: LogItem,
	index: VaultIndex,
	report: (line: number, rule: string, message: string) => void,
): void {
	if (!TIME_PREFIX_PATTERN.test(item.text))
		report(
			item.line,
			'time-prefix',
			'log line must start with a zero-padded HH:MM',
		);
	if (item.text.includes('|'))
		report(item.line, 'pipe', 'log line must not contain |');
	if (UNSPACED_FIELD_PATTERN.test(item.text))
		report(
			item.line,
			'field-spacing',
			'inline fields must be written [key:: value]',
		);
	const fields = [...item.text.matchAll(INLINE_FIELD_PATTERN)].map((match) => ({
		key: match[1] ?? '',
		value: (match[2] ?? '').trim(),
	}));
	const values = (key: string) =>
		fields.filter((field) => field.key === key).map((field) => field.value);
	lintKind(item, values('kind'), report);
	lintReference(item, values('story'), values('project'), index, report);
	for (const taskId of values('task')) {
		if (!TASK_ID_PATTERN.test(taskId) || !index.taskIds.has(taskId))
			report(item.line, 'task', `task ${taskId} does not exist in the vault`);
	}
	for (const agent of values('agent')) {
		if (agent.length === 0 || /\s/.test(agent))
			report(item.line, 'agent', 'agent token must be a single word');
	}
}

function lintKind(
	item: LogItem,
	kinds: string[],
	report: (line: number, rule: string, message: string) => void,
): void {
	if (kinds.length !== 1) {
		report(
			item.line,
			'kind',
			`log line must carry exactly one [kind:: …], found ${kinds.length}`,
		);
		return;
	}
	const kind = kinds[0] ?? '';
	if (!LOG_KINDS.some((allowed) => allowed === kind))
		report(
			item.line,
			'kind',
			`unknown kind ${kind}; allowed: ${LOG_KINDS.join(' | ')}`,
		);
}

function lintReference(
	item: LogItem,
	stories: string[],
	projects: string[],
	index: VaultIndex,
	report: (line: number, rule: string, message: string) => void,
): void {
	if (stories.length + projects.length > 1)
		report(
			item.line,
			'reference',
			'at most one [story:: …] or [project:: …] per log line',
		);
	for (const story of stories) {
		if (!STORY_NUMBER_PATTERN.test(story))
			report(
				item.line,
				'story',
				`story must look like STRY0000000, got ${story}`,
			);
		else if (!index.storyNumbers.has(story))
			report(item.line, 'story', `story ${story} has no note with that number`);
	}
	for (const project of projects) {
		if (!index.projectNames.has(project))
			report(
				item.line,
				'project',
				`project ${project} has no type: project hub`,
			);
	}
}
