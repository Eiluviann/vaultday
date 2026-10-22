import { createHash } from 'node:crypto';
import {
	existsSync,
	mkdirSync,
	readFileSync,
	realpathSync,
	writeFileSync,
} from 'node:fs';
import { homedir } from 'node:os';
import path from 'node:path';

export const DEFAULT_ID_STATE_DIRECTORY = path.join(
	homedir(),
	'.local',
	'state',
	'vaultday',
	'ids',
);

type IdState = { vault: string; next: Record<string, number> };

/**
 * Hands out `kind-N` ids for one vault. The counter lives in
 * `<stateDirectory>/<sha1 of the vault's realpath>.json`; a dry run allocates in
 * memory only and never persists.
 */
export class IdAllocator {
	readonly #statePath: string;
	readonly #vaultRealPath: string;
	readonly #next: Map<string, number>;
	readonly #allocated: string[] = [];

	constructor(
		vaultRoot: string,
		stateDirectory: string = DEFAULT_ID_STATE_DIRECTORY,
	) {
		this.#vaultRealPath = realpathSync(vaultRoot);
		this.#statePath = path.join(
			stateDirectory,
			`${createHash('sha1').update(this.#vaultRealPath).digest('hex')}.json`,
		);
		this.#next = readState(this.#statePath);
	}

	get statePath(): string {
		return this.#statePath;
	}

	get allocated(): readonly string[] {
		return this.#allocated;
	}

	peek(kind = 'task'): number {
		return this.#next.get(kind) ?? 1;
	}

	allocate(kind = 'task'): string {
		const counter = this.peek(kind);
		this.#next.set(kind, counter + 1);
		const id = `${kind}-${counter}`;
		this.#allocated.push(id);
		return id;
	}

	/** Writes the counters so the next run continues after the ids handed out here. */
	persist(): void {
		mkdirSync(path.dirname(this.#statePath), { recursive: true });
		const state: IdState = {
			vault: this.#vaultRealPath,
			next: Object.fromEntries(this.#next),
		};
		writeFileSync(this.#statePath, `${JSON.stringify(state, null, '\t')}\n`);
	}
}

function readState(statePath: string): Map<string, number> {
	if (!existsSync(statePath)) return new Map();
	const parsed: unknown = JSON.parse(readFileSync(statePath, 'utf8'));
	if (
		typeof parsed !== 'object' ||
		parsed === null ||
		!('next' in parsed) ||
		typeof parsed.next !== 'object' ||
		parsed.next === null
	) {
		throw new Error(`id state at ${statePath} is malformed`);
	}
	const next = new Map<string, number>();
	for (const [kind, counter] of Object.entries(parsed.next)) {
		if (
			typeof counter === 'number' &&
			Number.isInteger(counter) &&
			counter >= 1
		)
			next.set(kind, counter);
	}
	return next;
}
