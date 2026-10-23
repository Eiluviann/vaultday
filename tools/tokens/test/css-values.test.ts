import { describe, expect, it } from 'vitest';
import {
	parseColorLiteral,
	parseEm,
	parseFirstFontFamily,
	parsePixels,
	parseShadowList,
	toSwiftFontWeight,
} from '../src/css-values.ts';

describe('parseColorLiteral', () => {
	it('parses #rrggbb as opaque', () => {
		expect(parseColorLiteral('#e9ebee')).toEqual({
			ok: true,
			value: { red: 0xe9, green: 0xeb, blue: 0xee, alpha: 1 },
		});
	});

	it('parses #rrggbbaa, rounding alpha to three decimals', () => {
		expect(parseColorLiteral('#1b26330d')).toEqual({
			ok: true,
			value: { red: 27, green: 38, blue: 51, alpha: 0.051 },
		});
	});

	it('parses rgba() and rgb()', () => {
		expect(parseColorLiteral('rgba(27, 38, 51, 0.12)')).toEqual({
			ok: true,
			value: { red: 27, green: 38, blue: 51, alpha: 0.12 },
		});
		expect(parseColorLiteral('rgb(0,0,0)')).toEqual({
			ok: true,
			value: { red: 0, green: 0, blue: 0, alpha: 1 },
		});
	});

	it.each([
		'#fff',
		'#12345',
		'red',
		'rgba(256, 0, 0, 1)',
		'rgba(0, 0, 0, 1.5)',
	])('rejects %s', (text) => {
		expect(parseColorLiteral(text).ok).toBe(false);
	});
});

describe('lengths', () => {
	it('parses px and em values', () => {
		expect(parsePixels('20px')).toEqual({ ok: true, value: 20 });
		expect(parseEm('-0.01em')).toEqual({ ok: true, value: -0.01 });
	});

	it('rejects values in the wrong unit', () => {
		expect(parsePixels('20').ok).toBe(false);
		expect(parsePixels('2rem').ok).toBe(false);
		expect(parseEm('4px').ok).toBe(false);
	});
});

describe('toSwiftFontWeight', () => {
	it('maps 100 to 900 onto Font.Weight members', () => {
		const names = [100, 200, 300, 400, 500, 600, 700, 800, 900].map(
			(weight) => {
				const result = toSwiftFontWeight(weight);
				return result.ok ? result.value : result.error;
			},
		);
		expect(names).toEqual([
			'ultraLight',
			'thin',
			'light',
			'regular',
			'medium',
			'semibold',
			'bold',
			'heavy',
			'black',
		]);
	});

	it('rejects weights off the scale', () => {
		expect(toSwiftFontWeight(450).ok).toBe(false);
		expect(toSwiftFontWeight(1000).ok).toBe(false);
	});
});

describe('parseFirstFontFamily', () => {
	it('takes the first quoted family of a stack', () => {
		expect(
			parseFirstFontFamily('"Geist Mono", ui-monospace, monospace'),
		).toEqual({ ok: true, value: 'Geist Mono' });
	});

	it('fails when no family is quoted', () => {
		expect(parseFirstFontFamily('system-ui, sans-serif').ok).toBe(false);
	});
});

describe('parseShadowList', () => {
	it('parses a 4-part layer (x y blur color) with zero spread', () => {
		expect(parseShadowList('0 1px 2px rgba(27,38,51,0.12)')).toEqual({
			ok: true,
			value: [
				{
					x: 0,
					y: 1,
					blur: 2,
					spread: 0,
					color: { red: 27, green: 38, blue: 51, alpha: 0.12 },
				},
			],
		});
	});

	it('parses 5-part layers with hex colors and several layers', () => {
		const result = parseShadowList('0 0 0 2px #ffffff, 0 0 0 4px #c25e00');
		expect(result.ok).toBe(true);
		if (!result.ok) return;
		expect(result.value.map(({ spread }) => spread)).toEqual([2, 4]);
		expect(result.value[1]?.color).toEqual({
			red: 0xc2,
			green: 0x5e,
			blue: 0,
			alpha: 1,
		});
	});

	it('keeps commas inside rgba() within their layer', () => {
		const result = parseShadowList(
			'0 12px 32px rgba(27, 38, 51, 0.18), 0 0 0 1px rgba(27, 38, 51, 0.08)',
		);
		expect(result.ok && result.value.length).toBe(2);
	});

	it.each([
		'',
		'0 1px rgba(0,0,0,0.1)',
		'inset 0 1px 2px rgba(0,0,0,0.1)',
		'0 1px 2px 3px 4px #000000',
		'0 1px 2px, ',
		'0 1px 2px nope',
	])('rejects the malformed list "%s"', (text) => {
		expect(parseShadowList(text).ok).toBe(false);
	});
});
