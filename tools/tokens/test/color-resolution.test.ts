import { describe, expect, it } from 'vitest';
import { resolveColorTokens } from '../src/color-resolution.ts';
import type { ColorToken } from '../src/token-schema.ts';

function colorToken(name: string, value: ColorToken['value']): ColorToken {
	return { name, value, usage: `${name} usage` };
}

describe('resolveColorTokens', () => {
	it('uses a plain string value for both themes', () => {
		const result = resolveColorTokens([colorToken('white', '#ffffff')]);
		expect(result.ok && result.value[0]).toMatchObject({
			light: { red: 255 },
			dark: { red: 255 },
		});
	});

	it('resolves an alias to the same theme of its target, recursively', () => {
		const result = resolveColorTokens([
			colorToken('base', { light: '#000000', dark: '#ffffff' }),
			colorToken('middle', '{base}'),
			colorToken('top', { light: '{middle}', dark: '#0000ff' }),
		]);
		expect(result.ok).toBe(true);
		if (!result.ok) return;
		const top = result.value[2];
		expect(top?.light.red).toBe(0);
		expect(top?.dark.blue).toBe(255);
		expect(result.value[1]?.dark.red).toBe(255);
	});

	it('reports an alias to a missing token', () => {
		const result = resolveColorTokens([colorToken('a', '{nope}')]);
		expect(result.ok).toBe(false);
		if (!result.ok) {
			expect(result.error).toContain('missing token "nope"');
			expect(result.error).toContain('color "a"');
		}
	});

	it('reports an alias cycle, including a self-alias', () => {
		const cycle = resolveColorTokens([
			colorToken('a', '{b}'),
			colorToken('b', '{a}'),
		]);
		expect(cycle.ok).toBe(false);
		if (!cycle.ok) expect(cycle.error).toContain('a -> b -> a');

		expect(resolveColorTokens([colorToken('a', '{a}')]).ok).toBe(false);
	});

	it('reports an unparseable literal with the theme and token', () => {
		const result = resolveColorTokens([
			colorToken('a', { light: '#ffffff', dark: 'blue' }),
		]);
		expect(result.ok).toBe(false);
		if (!result.ok) expect(result.error).toContain('dark');
	});
});
