import path from 'node:path';
import { describe, expect, it } from 'vitest';
import { generateKeymapDoc, readText } from '../src/generate.ts';
import { capsOfSequence } from '../src/key-caps.ts';
import { buildKeyTable, parseKeymapEntries } from '../src/keymap.ts';
import {
	KEYMAP_DOC_PATH,
	KEYMAP_NOTES_PATH,
	KEYMAP_PATH,
	REPOSITORY_ROOT,
} from '../src/paths.ts';

describe('apps/apple/KEYMAP.md', () => {
	it('is the key table of the committed Keymap.yaml', () => {
		const generated = generateKeymapDoc(
			path.resolve(REPOSITORY_ROOT, KEYMAP_PATH),
			path.resolve(REPOSITORY_ROOT, KEYMAP_NOTES_PATH),
		);
		const committed = readText(path.resolve(REPOSITORY_ROOT, KEYMAP_DOC_PATH));
		expect(generated.ok && committed.ok).toBe(true);
		if (!generated.ok || !committed.ok) return;
		expect(committed.value).toBe(generated.value);
	});
});

describe('capsOfSequence', () => {
	it.each([
		['shift+g', 'G'],
		['alt+cmd+t', '⌥⌘T'],
		['shift+}', '}'],
		['> >', '> >'],
		['ctrl+shift+tab', '⌃⇧⇥'],
		['return', '⏎'],
		['g shift+t', 'g T'],
		['\\', '\\'],
		['cmd+k', '⌘K'],
		['cmd+left-of-1', '⌘§'],
	])('prints %s as %s', (notation, caps) => {
		expect(capsOfSequence(notation)).toEqual({ ok: true, value: caps });
	});

	it('refuses an unknown modifier or key name', () => {
		expect(capsOfSequence('hyper+k').ok).toBe(false);
		expect(capsOfSequence('cmd+enter').ok).toBe(false);
	});
});

describe('buildKeyTable', () => {
	it('keeps one row per action with the first label and every sequence', () => {
		const text =
			'version: 1\nbindings:\n  - action: palette.open\n    label: "Command palette" # the palette\n    group: "Navigation"\n    vim: "cmd+k"\n    standard: "cmd+k"\n  - action: palette.open\n    vim: ":"\n    standard: ""\n';
		const entries = parseKeymapEntries(text);
		expect(entries.ok).toBe(true);
		if (!entries.ok) return;
		expect(buildKeyTable(entries.value)).toEqual({
			ok: true,
			value: [
				{
					action: 'palette.open',
					label: 'Command palette',
					group: 'Navigation',
					vimCaps: ['⌘K', ':'],
					standardCaps: ['⌘K'],
				},
			],
		});
	});

	it('reports the line of a field it does not know', () => {
		const entries = parseKeymapEntries(
			'version: 1\nbindings:\n  - action: open\n    colour: "x"\n',
		);
		expect(entries).toEqual({
			ok: false,
			error: 'line 4: unknown field `colour`',
		});
	});
});
