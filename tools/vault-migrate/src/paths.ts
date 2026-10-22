import path from 'node:path';

/** Forward-slash, NFC-normalised vault-relative path for stable comparisons across APFS/iCloud spellings. */
export function normalisePath(relativePath: string): string {
	return relativePath.split(path.sep).join('/').normalize('NFC');
}

export function basenameWithoutExtension(filePath: string): string {
	const base = path.posix.basename(filePath);
	const extension = path.posix.extname(base);
	return extension.length > 0 ? base.slice(0, -extension.length) : base;
}

export function parentDirectory(relativePath: string): string {
	const parent = path.posix.dirname(relativePath);
	return parent === '.' ? '' : parent;
}

export function joinRelative(directory: string, name: string): string {
	return directory.length === 0 ? name : `${directory}/${name}`;
}

export function isUnderDirectory(
	relativePath: string,
	directory: string,
): boolean {
	return relativePath === directory || relativePath.startsWith(`${directory}/`);
}

export function hasPathSegment(relativePath: string, segment: string): boolean {
	return relativePath.split('/').slice(0, -1).includes(segment);
}

export function isSameCaseInsensitive(left: string, right: string): boolean {
	return left.toLowerCase() === right.toLowerCase();
}
