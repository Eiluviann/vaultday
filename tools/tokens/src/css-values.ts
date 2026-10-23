import { collectResults, err, ok, type Result } from './result.ts';

export type Rgba = {
	red: number;
	green: number;
	blue: number;
	alpha: number;
};

export type ShadowLayerValue = {
	x: number;
	y: number;
	blur: number;
	spread: number;
	color: Rgba;
};

export type SwiftFontWeight =
	| 'ultraLight'
	| 'thin'
	| 'light'
	| 'regular'
	| 'medium'
	| 'semibold'
	| 'bold'
	| 'heavy'
	| 'black';

const SWIFT_FONT_WEIGHT_BY_CSS_WEIGHT = new Map<number, SwiftFontWeight>([
	[100, 'ultraLight'],
	[200, 'thin'],
	[300, 'light'],
	[400, 'regular'],
	[500, 'medium'],
	[600, 'semibold'],
	[700, 'bold'],
	[800, 'heavy'],
	[900, 'black'],
]);

const PIXELS_PATTERN = /^(-?\d+(?:\.\d+)?)px$/;
const EM_PATTERN = /^(-?\d+(?:\.\d+)?)em$/;
const MILLISECONDS_PATTERN = /^(\d+(?:\.\d+)?)ms$/;
const SHADOW_LENGTH_PATTERN = /^(-?\d+(?:\.\d+)?)(?:px)?$/;
const HEX_COLOR_PATTERN = /^#([0-9a-fA-F]{6})([0-9a-fA-F]{2})?$/;
const FUNCTION_COLOR_PATTERN =
	/^rgba?\(\s*(\d{1,3})\s*,\s*(\d{1,3})\s*,\s*(\d{1,3})\s*(?:,\s*(\d*\.?\d+)\s*)?\)$/;
const QUOTED_FAMILY_PATTERN = /["']([^"']+)["']/;

/** Parses a CSS length such as `20px`. */
export function parsePixels(text: string): Result<number> {
	return parseMeasure(text, PIXELS_PATTERN, 'a length like "20px"');
}

/** Parses a CSS duration in milliseconds such as `150ms`. */
export function parseMilliseconds(text: string): Result<number> {
	return parseMeasure(text, MILLISECONDS_PATTERN, 'a duration like "150ms"');
}

/** Parses a CSS letter spacing such as `-0.01em`. */
export function parseEm(text: string): Result<number> {
	return parseMeasure(text, EM_PATTERN, 'a length like "0.04em"');
}

/** Parses `#rrggbb`, `#rrggbbaa`, `rgb(r, g, b)` or `rgba(r, g, b, a)`. */
export function parseColorLiteral(text: string): Result<Rgba> {
	const trimmed = text.trim();
	const hexMatch = HEX_COLOR_PATTERN.exec(trimmed);
	const rgbHex = hexMatch?.[1];
	if (rgbHex !== undefined) return ok(rgbaFromHex(rgbHex, hexMatch?.[2]));
	const functionMatch = FUNCTION_COLOR_PATTERN.exec(trimmed);
	if (functionMatch) return rgbaFromFunctionMatch(functionMatch);
	return err(`expected #rrggbb, #rrggbbaa or rgba(...), got "${text}"`);
}

/** Maps a CSS font weight (100 to 900) to the SwiftUI `Font.Weight` member. */
export function toSwiftFontWeight(cssWeight: number): Result<SwiftFontWeight> {
	const weight = SWIFT_FONT_WEIGHT_BY_CSS_WEIGHT.get(cssWeight);
	if (weight === undefined) {
		return err(`unsupported font weight ${cssWeight} (use 100 to 900)`);
	}
	return ok(weight);
}

/** Returns the first quoted family of a CSS font stack. */
export function parseFirstFontFamily(fontStack: string): Result<string> {
	const familyName = QUOTED_FAMILY_PATTERN.exec(fontStack)?.[1];
	if (familyName === undefined) {
		return err(`no quoted font family in "${fontStack}"`);
	}
	return ok(familyName);
}

/** Parses a CSS box-shadow list of `x y blur [spread] color` layers. */
export function parseShadowList(text: string): Result<ShadowLayerValue[]> {
	const layerTexts = splitOutsideParentheses(text, (char) => char === ',');
	return collectResults(layerTexts, parseShadowLayer);
}

function parseShadowLayer(layerText: string): Result<ShadowLayerValue> {
	const parts = splitOutsideParentheses(layerText, isWhitespace).filter(
		(part) => part !== '',
	);
	if (parts.length < 4 || parts.length > 5) {
		return err(
			`shadow layer "${layerText.trim()}" must be "x y blur [spread] color"`,
		);
	}
	const colorText = parts[parts.length - 1] ?? '';
	const lengthsResult = collectResults(parts.slice(0, -1), parseShadowLength);
	if (!lengthsResult.ok) return lengthsResult;
	const colorResult = parseColorLiteral(colorText);
	if (!colorResult.ok) return colorResult;
	const [x, y, blur, spread = 0] = lengthsResult.value;
	if (x === undefined || y === undefined || blur === undefined) {
		return err(`shadow layer "${layerText.trim()}" lacks x, y or blur`);
	}
	return ok({ x, y, blur, spread, color: colorResult.value });
}

function parseShadowLength(text: string): Result<number> {
	return parseMeasure(text, SHADOW_LENGTH_PATTERN, 'a length like "12px"');
}

function parseMeasure(
	text: string,
	pattern: RegExp,
	expectation: string,
): Result<number> {
	const numberText = pattern.exec(text.trim())?.[1];
	if (numberText === undefined)
		return err(`expected ${expectation}, got "${text}"`);
	return ok(Number(numberText));
}

function rgbaFromHex(rgbHex: string, alphaHex: string | undefined): Rgba {
	return {
		red: Number.parseInt(rgbHex.slice(0, 2), 16),
		green: Number.parseInt(rgbHex.slice(2, 4), 16),
		blue: Number.parseInt(rgbHex.slice(4, 6), 16),
		alpha:
			alphaHex === undefined
				? 1
				: roundToThousandths(Number.parseInt(alphaHex, 16) / 255),
	};
}

function rgbaFromFunctionMatch(match: RegExpExecArray): Result<Rgba> {
	const [, redText, greenText, blueText, alphaText] = match;
	if (
		redText === undefined ||
		greenText === undefined ||
		blueText === undefined
	) {
		return err(`incomplete color function "${match[0]}"`);
	}
	const [red, green, blue] = [redText, greenText, blueText].map(Number);
	const alpha = alphaText === undefined ? 1 : Number(alphaText);
	if (red === undefined || green === undefined || blue === undefined) {
		return err(`incomplete color function "${match[0]}"`);
	}
	if (Math.max(red, green, blue) > 255) {
		return err(`color channel above 255 in "${match[0]}"`);
	}
	if (alpha > 1) return err(`alpha above 1 in "${match[0]}"`);
	return ok({ red, green, blue, alpha });
}

function roundToThousandths(value: number): number {
	return Math.round(value * 1000) / 1000;
}

function isWhitespace(char: string): boolean {
	return /\s/.test(char);
}

/** Splits on a separator that is not nested inside parentheses. */
function splitOutsideParentheses(
	text: string,
	isSeparator: (char: string) => boolean,
): string[] {
	const segments: string[] = [];
	let depth = 0;
	let current = '';
	for (const char of text) {
		if (char === '(') depth += 1;
		if (char === ')') depth -= 1;
		if (depth === 0 && isSeparator(char)) {
			segments.push(current);
			current = '';
		} else {
			current += char;
		}
	}
	segments.push(current);
	return segments;
}
