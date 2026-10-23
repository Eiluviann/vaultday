import { describe, expect, it } from 'vitest';
import { toSwiftIdentifier } from '../src/swift-naming.ts';

describe('toSwiftIdentifier', () => {
	it.each([
		['surface-window', 'surfaceWindow'],
		['space-2', 'space2'],
		['title-large', 'titleLarge'],
		['on-ws', 'onWs'],
		['ink', 'ink'],
		['size-control-sm', 'sizeControlSm'],
	])('camelCases %s to %s', (tokenName, expected) => {
		expect(toSwiftIdentifier(tokenName)).toEqual({ ok: true, value: expected });
	});

	it('rejects names that would start with a digit', () => {
		const result = toSwiftIdentifier('2-up');
		expect(result.ok).toBe(false);
		if (!result.ok) expect(result.error).toContain('digit');
	});

	it.each(['default', 'self', 'class', 'in'])(
		'rejects the Swift keyword %s',
		(tokenName) => {
			const result = toSwiftIdentifier(tokenName);
			expect(result.ok).toBe(false);
			if (!result.ok) expect(result.error).toContain('keyword');
		},
	);

	it.each(['Surface', 'a--b', 'a_b', '-a', 'a-', ''])(
		'rejects the non-kebab-case name "%s"',
		(tokenName) => {
			expect(toSwiftIdentifier(tokenName).ok).toBe(false);
		},
	);
});
