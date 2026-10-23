import { capsOfSequence } from './key-caps.ts';
import { err, ok, type Result } from './result.ts';

/** One `- action:` entry of Keymap.yaml, its fields as written. */
export type KeymapEntry = {
	line: number;
	fields: ReadonlyMap<string, string>;
};

/** One row of the key table: an action with every sequence that runs it. */
export type KeyTableRow = {
	action: string;
	label: string;
	group: string;
	vimCaps: string[];
	standardCaps: string[];
};

type MutableEntry = { line: number; fields: Map<string, string> };

const DEFAULT_GROUP = 'General';
const KNOWN_FIELDS: ReadonlySet<string> = new Set([
	'action',
	'vim',
	'standard',
	'label',
	'group',
	'tip',
	'menu',
	'pane',
]);

/**
 * Reads the entries of Keymap.yaml with the same small grammar the app's
 * `Keymap.parse` reads: top-level `version` and `bindings`, `- key: value`
 * entries, `#` comments outside quotes.
 *
 * @param text - The file's text.
 * @returns The entries in file order, or the first line that does not read.
 */
export function parseKeymapEntries(text: string): Result<KeymapEntry[]> {
	const entries: MutableEntry[] = [];
	let isInBindings = false;
	for (const [index, rawLine] of text.split(/\r?\n/).entries()) {
		const lineNumber = index + 1;
		const line = withoutComment(rawLine);
		if (line.trim() === '') continue;
		const read: Result<boolean> = line.startsWith(' ')
			? readBindingLine(entries, line.trim(), lineNumber, isInBindings)
			: readTopLevelLine(line.trim(), lineNumber);
		if (!read.ok) return read;
		isInBindings ||= read.value;
	}
	return ok(entries);
}

/** A top-level line; `true` when it opens `bindings:`. */
function readTopLevelLine(
	trimmed: string,
	lineNumber: number,
): Result<boolean> {
	const field = splitField(trimmed);
	if (field === undefined)
		return err(`line ${lineNumber}: expected \`key: value\``);
	if (field.key === 'bindings' && field.value === '') return ok(true);
	if (field.key === 'version') return ok(false);
	return err(`line ${lineNumber}: unknown top-level key \`${field.key}\``);
}

/** An indented line: a new entry's first field, or one more field of the last entry. */
function readBindingLine(
	entries: MutableEntry[],
	trimmed: string,
	lineNumber: number,
	isInBindings: boolean,
): Result<boolean> {
	if (!isInBindings)
		return err(`line ${lineNumber}: indented line outside \`bindings:\``);
	const isEntryStart = trimmed.startsWith('- ');
	if (isEntryStart) entries.push({ line: lineNumber, fields: new Map() });
	const field = splitField(isEntryStart ? trimmed.slice(2) : trimmed);
	const entry = entries.at(-1);
	if (field === undefined || entry === undefined)
		return err(
			`line ${lineNumber}: expected \`- action: …\` or \`key: value\``,
		);
	if (!KNOWN_FIELDS.has(field.key))
		return err(`line ${lineNumber}: unknown field \`${field.key}\``);
	entry.fields.set(field.key, field.value);
	return ok(false);
}

/**
 * The key table: one row per action in file order, its label from the
 * first entry that has one, every sequence under each scheme.
 *
 * @param entries - The parsed entries.
 * @returns The rows, or why an entry's keys do not read.
 */
export function buildKeyTable(
	entries: readonly KeymapEntry[],
): Result<KeyTableRow[]> {
	const rows = new Map<string, KeyTableRow>();
	for (const entry of entries) {
		const action = entry.fields.get('action');
		if (action === undefined)
			return err(`line ${entry.line}: a binding without \`action\``);
		const row = rows.get(action) ?? {
			action,
			label: '',
			group: nonEmpty(entry.fields.get('group')) ?? DEFAULT_GROUP,
			vimCaps: [],
			standardCaps: [],
		};
		if (row.label === '') row.label = nonEmpty(entry.fields.get('label')) ?? '';
		const vim = appendCaps(row.vimCaps, entry.fields.get('vim'), entry.line);
		if (!vim.ok) return vim;
		const standard = appendCaps(
			row.standardCaps,
			entry.fields.get('standard'),
			entry.line,
		);
		if (!standard.ok) return standard;
		rows.set(action, row);
	}
	return ok([...rows.values()]);
}

/**
 * The key table as the Markdown page `apps/apple/KEYMAP.md`.
 *
 * @param rows - The key table.
 * @returns The page, ending in a newline.
 */
export function renderKeymapMarkdown(rows: readonly KeyTableRow[]): string {
	const header = [
		'# Vaultday keys',
		'',
		'Generated from `apps/apple/Keymap.yaml` by `tools/keymap-doc`; do not edit by hand. Run `pnpm --filter @vaultday/keymap-doc generate` after changing the keymap.',
		'',
		'| Group | Action | Label | Vim motions | Standard |',
		'| --- | --- | --- | --- | --- |',
	];
	const body = rows.map(
		(row) =>
			`| ${row.group} | \`${row.action}\` | ${row.label} | ${capsCell(row.vimCaps)} | ${capsCell(row.standardCaps)} |`,
	);
	return `${[...header, ...body].join('\n')}\n`;
}

function appendCaps(
	target: string[],
	notation: string | undefined,
	line: number,
): Result<void> {
	if (notation === undefined || notation.trim() === '') return ok(undefined);
	const caps = capsOfSequence(notation);
	if (!caps.ok) return err(`line ${line}: ${caps.error}`);
	target.push(caps.value);
	return ok(undefined);
}

function capsCell(caps: readonly string[]): string {
	return caps.length === 0 ? '—' : caps.map((cap) => `\`${cap}\``).join(' · ');
}

function nonEmpty(text: string | undefined): string | undefined {
	return text === undefined || text === '' ? undefined : text;
}

function withoutComment(line: string): string {
	let quote: string | undefined;
	for (const [offset, character] of [...line].entries()) {
		if (character === '"' || character === "'") {
			if (quote === undefined) quote = character;
			else if (quote === character) quote = undefined;
		}
		if (character === '#' && quote === undefined)
			return [...line].slice(0, offset).join('');
	}
	return line;
}

function splitField(text: string): { key: string; value: string } | undefined {
	const colon = text.indexOf(':');
	if (colon === -1) return undefined;
	const key = text.slice(0, colon).trim();
	let value = text.slice(colon + 1).trim();
	for (const quote of ['"', "'"]) {
		if (value.length >= 2 && value.startsWith(quote) && value.endsWith(quote)) {
			value = value.slice(1, -1);
			break;
		}
	}
	return key === '' ? undefined : { key, value };
}
