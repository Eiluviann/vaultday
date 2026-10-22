import { isUnderDirectory } from './paths.ts';

export type PathMapping = {
	from: string;
	/** New vault-relative path, or undefined when the file leaves the vault. */
	to: string | undefined;
	kind: 'file' | 'directory';
};

/** Old → new vault-relative paths from renames and moves; directory entries cover everything below them. */
export class PathMap {
	readonly #mappings: PathMapping[] = [];

	add(mapping: PathMapping): void {
		this.#mappings.push(mapping);
	}

	get mappings(): readonly PathMapping[] {
		return this.#mappings;
	}

	/** Final location of a current path; undefined when it leaves the vault. */
	map(relativePath: string): string | undefined {
		let current = relativePath;
		for (const mapping of this.#mappings) {
			if (mapping.kind === 'file') {
				if (current === mapping.from) current = mapping.to ?? '';
				continue;
			}
			if (isUnderDirectory(current, mapping.from)) {
				if (mapping.to === undefined) return undefined;
				current =
					current === mapping.from
						? mapping.to
						: `${mapping.to}${current.slice(mapping.from.length)}`;
			}
		}
		return current.length === 0 ? undefined : current;
	}

	get isEmpty(): boolean {
		return this.#mappings.length === 0;
	}
}
