import { err, ok, type Result } from './result.ts';

const KEBAB_CASE_PATTERN = /^[a-z0-9]+(?:-[a-z0-9]+)*$/;

const SWIFT_KEYWORDS: ReadonlySet<string> = new Set([
	'Any',
	'Self',
	'_',
	'as',
	'associatedtype',
	'await',
	'break',
	'case',
	'catch',
	'class',
	'continue',
	'default',
	'defer',
	'deinit',
	'do',
	'else',
	'enum',
	'extension',
	'fallthrough',
	'false',
	'fileprivate',
	'for',
	'func',
	'guard',
	'if',
	'import',
	'in',
	'init',
	'inout',
	'internal',
	'is',
	'let',
	'nil',
	'open',
	'operator',
	'precedencegroup',
	'private',
	'protocol',
	'public',
	'repeat',
	'rethrows',
	'return',
	'self',
	'static',
	'struct',
	'subscript',
	'super',
	'switch',
	'throw',
	'throws',
	'true',
	'try',
	'typealias',
	'var',
	'where',
	'while',
]);

/**
 * Turns a kebab-case token name into a Swift identifier (`surface-window` to
 * `surfaceWindow`, `space-2` to `space2`).
 * Fails for names that are not kebab-case, would start with a digit, or clash
 * with a Swift keyword.
 */
export function toSwiftIdentifier(tokenName: string): Result<string> {
	if (/^\d/.test(tokenName)) {
		return err(`"${tokenName}" would start with a digit in Swift`);
	}
	if (!KEBAB_CASE_PATTERN.test(tokenName)) {
		return err(`"${tokenName}" is not kebab-case (a-z, 0-9, single dashes)`);
	}
	const identifier = tokenName
		.split('-')
		.map(capitalizeAfterFirstSegment)
		.join('');
	if (SWIFT_KEYWORDS.has(identifier)) {
		return err(`"${tokenName}" clashes with the Swift keyword "${identifier}"`);
	}
	return ok(identifier);
}

function capitalizeAfterFirstSegment(segment: string, index: number): string {
	if (index === 0) return segment;
	return segment.charAt(0).toUpperCase() + segment.slice(1);
}
