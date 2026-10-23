import { readFileSync } from 'node:fs';
import {
	buildKeyTable,
	parseKeymapEntries,
	renderKeymapMarkdown,
} from './keymap.ts';
import { err, ok, type Result } from './result.ts';

/**
 * Reads Keymap.yaml at `keymapPath` and renders the key table page.
 *
 * @param keymapPath - Absolute path of Keymap.yaml.
 * @returns The page, or why the keymap could not be read or rendered.
 */
export function generateKeymapDoc(keymapPath: string): Result<string> {
	const text = readText(keymapPath);
	if (!text.ok) return text;
	const entries = parseKeymapEntries(text.value);
	if (!entries.ok) return entries;
	const rows = buildKeyTable(entries.value);
	if (!rows.ok) return rows;
	return ok(renderKeymapMarkdown(rows.value));
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
