import { err, ok, type Result } from './result.ts';

type Modifier = 'control' | 'option' | 'shift' | 'command';

type KeyStroke = {
	key: string;
	modifiers: ReadonlySet<Modifier>;
};

/** The symbols a US layout types with Shift; they carry Shift by themselves. */
const SHIFTED_SYMBOLS: ReadonlySet<string> = new Set([
	'~',
	'!',
	'@',
	'#',
	'$',
	'%',
	'^',
	'&',
	'*',
	'(',
	')',
	'_',
	'+',
	'{',
	'}',
	'|',
	':',
	'"',
	'<',
	'>',
	'?',
]);

const MODIFIER_NAMES: Readonly<Record<string, Modifier>> = {
	shift: 'shift',
	ctrl: 'control',
	opt: 'option',
	alt: 'option',
	cmd: 'command',
};

const NAMED_KEY_CAPS: Readonly<Record<string, string>> = {
	return: '⏎',
	escape: 'Esc',
	tab: '⇥',
	space: 'Space',
	up: '↑',
	down: '↓',
	left: '←',
	right: '→',
	pageup: '⇞',
	pagedown: '⇟',
	delete: '⌫',
};

const MODIFIER_CAPS: readonly (readonly [Modifier, string])[] = [
	['control', '⌃'],
	['option', '⌥'],
	['shift', '⇧'],
	['command', '⌘'],
];

/**
 * A sequence in Keymap.yaml's notation (`g b`, `alt+cmd+t`) as the key caps
 * the app prints, strokes joined by a space; the same rules as the Swift
 * `KeyStroke.capText`.
 *
 * @param notation - Strokes separated by spaces.
 * @returns The caps, or why the notation is not a key sequence.
 */
export function capsOfSequence(notation: string): Result<string> {
	const caps: string[] = [];
	for (const strokeNotation of notation
		.split(' ')
		.filter((part) => part !== '')) {
		const stroke = parseStroke(strokeNotation);
		if (!stroke.ok) return stroke;
		caps.push(capText(stroke.value));
	}
	return ok(caps.join(' '));
}

function parseStroke(notation: string): Result<KeyStroke> {
	const parts = notation.split('+').map((part) => part.trim().toLowerCase());
	const key = parts.at(-1);
	if (key === undefined || key === '')
		return err(`\`${notation}\` is not a key stroke`);
	const modifiers = new Set<Modifier>();
	for (const part of parts.slice(0, -1)) {
		const modifier = MODIFIER_NAMES[part];
		if (modifier === undefined)
			return err(`\`${part}\` is not a modifier in \`${notation}\``);
		modifiers.add(modifier);
	}
	if ([...key].length !== 1 && NAMED_KEY_CAPS[key] === undefined) {
		return err(`\`${key}\` is not a key name in \`${notation}\``);
	}
	if (SHIFTED_SYMBOLS.has(key)) modifiers.add('shift');
	return ok({ key, modifiers });
}

function capText(stroke: KeyStroke): string {
	const isSingleCharacter = [...stroke.key].length === 1;
	// Vim's own spelling for a shifted letter or symbol: `G`, `P`, `>`, `:`.
	if (
		isSingleCharacter &&
		stroke.modifiers.size === 1 &&
		stroke.modifiers.has('shift')
	) {
		return stroke.key.toUpperCase();
	}
	const symbols = MODIFIER_CAPS.filter(([modifier]) =>
		stroke.modifiers.has(modifier),
	)
		.filter(
			([modifier]) =>
				!(modifier === 'shift' && SHIFTED_SYMBOLS.has(stroke.key)),
		)
		.map(([, symbol]) => symbol);
	const name =
		NAMED_KEY_CAPS[stroke.key] ??
		(stroke.modifiers.size === 0 ? stroke.key : stroke.key.toUpperCase());
	return symbols.join('') + name;
}
