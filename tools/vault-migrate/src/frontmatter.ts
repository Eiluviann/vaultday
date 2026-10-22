/**
 * Line-preserving frontmatter editing. Entries keep their raw value lines so
 * untouched keys (Templater expressions, block lists, quoted strings) come out
 * byte-identical; only the keys a rule changes are re-rendered.
 */

export type FrontmatterPlacement =
	| 'start'
	| 'end'
	| { after: string }
	| { before: string };

type FrontmatterEntry = {
	key: string;
	/** Text after `key:` on the key line (usually ' value' or ''), followed by continuation lines. */
	valueLines: string[];
};

const KEY_LINE_PATTERN = /^([A-Za-z_][\w-]*):(.*)$/;
const INLINE_LIST_PATTERN = /^\s*\[(.*)\]\s*$/;
const BLOCK_ITEM_PATTERN = /^\s*-\s*(.*)$/;
const PLAIN_SCALAR_PATTERN = /^[A-Za-z0-9_][A-Za-z0-9_ .()\-/]*$/;

export class Frontmatter {
	readonly #entries: FrontmatterEntry[];
	readonly #preamble: string[];

	private constructor(entries: FrontmatterEntry[], preamble: string[]) {
		this.#entries = entries;
		this.#preamble = preamble;
	}

	/** Parses the lines between the `---` delimiters (delimiters excluded). */
	static parse(lines: string[]): Frontmatter {
		const entries: FrontmatterEntry[] = [];
		const preamble: string[] = [];
		for (const line of lines) {
			const match = KEY_LINE_PATTERN.exec(line);
			if (match !== null && match[1] !== undefined) {
				entries.push({ key: match[1], valueLines: [match[2] ?? ''] });
				continue;
			}
			const current = entries.at(-1);
			if (current === undefined) preamble.push(line);
			else current.valueLines.push(line);
		}
		return new Frontmatter(entries, preamble);
	}

	static empty(): Frontmatter {
		return new Frontmatter([], []);
	}

	get keys(): string[] {
		return this.#entries.map((entry) => entry.key);
	}

	get isEmpty(): boolean {
		return this.#entries.length === 0 && this.#preamble.length === 0;
	}

	has(key: string): boolean {
		return this.#find(key) !== undefined;
	}

	/** Inline scalar value, unquoted and trimmed; '' for an empty value; undefined when the key is absent. */
	scalar(key: string): string | undefined {
		const entry = this.#find(key);
		if (entry === undefined) return undefined;
		return unquote((entry.valueLines[0] ?? '').trim());
	}

	/** Items of an inline `[a, b]` or block `- a` list; a scalar value counts as a one-item list. */
	list(key: string): string[] {
		const entry = this.#find(key);
		if (entry === undefined) return [];
		const inline = (entry.valueLines[0] ?? '').trim();
		const inlineMatch = INLINE_LIST_PATTERN.exec(inline);
		if (inlineMatch !== null) return splitInlineList(inlineMatch[1] ?? '');
		if (inline.length > 0) return [unquote(inline)];
		return entry.valueLines
			.slice(1)
			.map((line) => BLOCK_ITEM_PATTERN.exec(line))
			.flatMap((match) =>
				match === null ? [] : [unquote((match[1] ?? '').trim())],
			)
			.filter((item) => item.length > 0);
	}

	rename(oldKey: string, newKey: string): void {
		const entry = this.#find(oldKey);
		if (entry === undefined || oldKey === newKey) return;
		if (this.has(newKey))
			throw new Error(`cannot rename ${oldKey}: ${newKey} already exists`);
		entry.key = newKey;
	}

	/** Sets a scalar value (re-rendered as `key: value`, or `key:` when empty), inserting the key when absent. */
	setScalar(
		key: string,
		value: string,
		placement: FrontmatterPlacement = 'end',
	): void {
		const rendered = value.length === 0 ? '' : ` ${quoteIfNeeded(value)}`;
		this.#upsert(key, [rendered], placement);
	}

	/** Sets a raw value exactly as given after `key:` (for values that must keep their original quoting). */
	setRaw(
		key: string,
		rawValue: string,
		placement: FrontmatterPlacement = 'end',
	): void {
		this.#upsert(key, [rawValue], placement);
	}

	setList(
		key: string,
		items: string[],
		placement: FrontmatterPlacement = 'end',
	): void {
		const rendered =
			items.length === 0 ? ' []' : ` [${items.map(quoteListItem).join(', ')}]`;
		this.#upsert(key, [rendered], placement);
	}

	remove(key: string): void {
		const index = this.#entries.findIndex((entry) => entry.key === key);
		if (index !== -1) this.#entries.splice(index, 1);
	}

	/** The block's lines without delimiters. */
	render(): string[] {
		return [
			...this.#preamble,
			...this.#entries.flatMap((entry) => [
				`${entry.key}:${entry.valueLines[0] ?? ''}`,
				...entry.valueLines.slice(1),
			]),
		];
	}

	#find(key: string): FrontmatterEntry | undefined {
		return this.#entries.find((entry) => entry.key === key);
	}

	#upsert(
		key: string,
		valueLines: string[],
		placement: FrontmatterPlacement,
	): void {
		const existing = this.#find(key);
		if (existing !== undefined) {
			existing.valueLines = valueLines;
			return;
		}
		const entry: FrontmatterEntry = { key, valueLines };
		const index = this.#placementIndex(placement);
		this.#entries.splice(index, 0, entry);
	}

	#placementIndex(placement: FrontmatterPlacement): number {
		if (placement === 'start') return 0;
		if (placement === 'end') return this.#entries.length;
		if ('after' in placement) {
			const index = this.#entries.findIndex(
				(entry) => entry.key === placement.after,
			);
			return index === -1 ? this.#entries.length : index + 1;
		}
		const index = this.#entries.findIndex(
			(entry) => entry.key === placement.before,
		);
		return index === -1 ? this.#entries.length : index;
	}
}

/** Splits `lines` into the frontmatter block (delimiters included) and the index where the body starts. */
export function splitFrontmatterBlock(lines: string[]): {
	blockEnd: number;
	frontmatter: Frontmatter;
} {
	if (lines[0] !== '---')
		return { blockEnd: 0, frontmatter: Frontmatter.empty() };
	for (let index = 1; index < lines.length; index++) {
		const line = lines[index] ?? '';
		if (line === '---' || line === '...')
			return {
				blockEnd: index + 1,
				frontmatter: Frontmatter.parse(lines.slice(1, index)),
			};
	}
	return { blockEnd: 0, frontmatter: Frontmatter.empty() };
}

export function quoteIfNeeded(value: string): string {
	if (
		PLAIN_SCALAR_PATTERN.test(value) &&
		!/^(true|false|null|yes|no|~)$/i.test(value) &&
		!/^[\d.]+$/.test(value)
	)
		return value;
	if (/^\d{4}-\d{2}-\d{2}$/.test(value) || /^\d+(\.\d+)?$/.test(value))
		return value;
	return `"${value.replaceAll('\\', '\\\\').replaceAll('"', '\\"')}"`;
}

/** Flow-sequence items are quoted unless they are a single bare token, so titles with spaces or brackets stay unambiguous. */
function quoteListItem(item: string): string {
	return /^[\w.-]+$/.test(item)
		? quoteIfNeeded(item)
		: `"${item.replaceAll('\\', '\\\\').replaceAll('"', '\\"')}"`;
}

function unquote(value: string): string {
	if (value.length >= 2 && value.startsWith('"') && value.endsWith('"'))
		return value.slice(1, -1).replaceAll('\\"', '"').replaceAll('\\\\', '\\');
	if (value.length >= 2 && value.startsWith("'") && value.endsWith("'"))
		return value.slice(1, -1).replaceAll("''", "'");
	return value;
}

function splitInlineList(inner: string): string[] {
	const items: string[] = [];
	let current = '';
	let quote: string | undefined;
	for (const character of inner) {
		if (quote !== undefined) {
			current += character;
			if (character === quote) quote = undefined;
			continue;
		}
		if (character === '"' || character === "'") {
			quote = character;
			current += character;
			continue;
		}
		if (character === ',') {
			items.push(current);
			current = '';
			continue;
		}
		current += character;
	}
	items.push(current);
	return items
		.map((item) => unquote(item.trim()))
		.filter((item) => item.length > 0);
}
