import { readFileSync } from 'node:fs';
import {
	buildKeyTable,
	parseKeymapEntries,
	renderKeymapMarkdown,
} from './keymap.ts';
import { err, ok, type Result } from './result.ts';

/**
 * Reads Keymap.yaml at `keymapPath` and renders the key table page, with
 * the hand-written notes at `notesPath` (focus, Esc, Vim inside fields)
 * after the table.
 *
 * @param keymapPath - Absolute path of Keymap.yaml.
 * @param notesPath - Absolute path of the notes page; none when omitted.
 * @returns The page, or why the keymap or the notes could not be read or rendered.
 */
export function generateKeymapDoc(
	keymapPath: string,
	notesPath?: string,
): Result<string> {
	const text = readText(keymapPath);
	if (!text.ok) return text;
	const entries = parseKeymapEntries(text.value);
	if (!entries.ok) return entries;
	const rows = buildKeyTable(entries.value);
	if (!rows.ok) return rows;
	const table = renderKeymapMarkdown(rows.value);
	if (notesPath === undefined) return ok(table);
	const notes = readText(notesPath);
	if (!notes.ok) return notes;
	return ok(`${table}\n${notes.value.trimEnd()}\n`);
}

/**
 * A file's text, or why it could not be read.
 *
 * @param filePath - Absolute path.
 */
export function readText(filePath: string): Result<string> {
	try {
		return ok(readFileSync(filePath, 'utf8'));
	} catch (error) {
		return err(error instanceof Error ? error.message : String(error));
	}
}
