import { resolveColorTokens } from './color-resolution.ts';
import type { ShadowLayerValue } from './css-values.ts';
import {
	parseEm,
	parseFirstFontFamily,
	parsePixels,
	parseShadowList,
	toSwiftFontWeight,
} from './css-values.ts';
import {
	collectResults,
	err,
	ok,
	type Result,
	withErrorContext,
} from './result.ts';
import {
	formatNumber,
	formatRgbaWithDecimalChannels,
	formatRgbaWithHexChannels,
	formatStringLiteral,
} from './swift-literals.ts';
import { toSwiftIdentifier } from './swift-naming.ts';
import type {
	ColorToken,
	LengthToken,
	ShadowToken,
	TokenFile,
	TypeStyle,
} from './token-schema.ts';

export type SwiftConstant = {
	identifier: string;
	usage: string;
	typeAnnotation: string | undefined;
	expression: string;
};

type TypeGroup = TokenFile['type']['groups'][number];

type TypographyEntry = { style: TypeStyle; familyName: string };

/** Builds the `Colors` constants: dynamic light/dark `Color` values. */
export function buildColorConstants(
	tokens: readonly ColorToken[],
): Result<SwiftConstant[]> {
	const resolved = resolveColorTokens(tokens);
	if (!resolved.ok) return resolved;
	return collectConstants('color', resolved.value, (color) =>
		defineConstant(
			color.name,
			color.usage,
			undefined,
			ok(
				`Color(light: ${formatRgbaWithHexChannels(color.light)}, dark: ${formatRgbaWithHexChannels(color.dark)})`,
			),
		),
	);
}

/** Builds `CGFloat` constants from `NNpx` length tokens (spacing, radius, size). */
export function buildLengthConstants(
	label: string,
	tokens: readonly LengthToken[],
): Result<SwiftConstant[]> {
	return collectConstants(label, tokens, (token) => {
		const pixels = parsePixels(token.value);
		const expression = pixels.ok ? ok(formatNumber(pixels.value)) : pixels;
		return defineConstant(token.name, token.usage, 'CGFloat', expression);
	});
}

/** Builds the `Typography` constants: one `TextStyle` per style, in group order. */
export function buildTypographyConstants(
	typeSection: TokenFile['type'],
): Result<SwiftConstant[]> {
	const entries = collectTypographyEntries(typeSection);
	if (!entries.ok) return entries;
	return collectConstants('type style', entries.value, (entry) =>
		defineConstant(
			entry.style.name,
			entry.style.usage,
			undefined,
			buildTextStyleExpression(entry),
		),
	);
}

/** Builds the `Shadows` constants: one `ShadowStyle` per theme pair. */
export function buildShadowConstants(
	tokens: readonly ShadowToken[],
): Result<SwiftConstant[]> {
	return collectConstants('shadow', tokens, (token) =>
		defineConstant(
			token.name,
			token.usage,
			undefined,
			buildShadowStyleExpression(token),
		),
	);
}

function collectTypographyEntries(
	typeSection: TokenFile['type'],
): Result<TypographyEntry[]> {
	const entriesPerGroup = collectResults(typeSection.groups, (group) =>
		collectGroupEntries(group, typeSection.families),
	);
	return entriesPerGroup.ok
		? ok(entriesPerGroup.value.flat())
		: entriesPerGroup;
}

function collectGroupEntries(
	group: TypeGroup,
	families: TokenFile['type']['families'],
): Result<TypographyEntry[]> {
	const fontStack = families[group.family];
	if (fontStack === undefined) {
		return err(`type group uses unknown family "${group.family}"`);
	}
	const familyName = parseFirstFontFamily(fontStack);
	if (!familyName.ok) {
		return withErrorContext(`family "${group.family}"`, familyName);
	}
	return ok(
		group.styles.map((style) => ({ style, familyName: familyName.value })),
	);
}

function buildTextStyleExpression(entry: TypographyEntry): Result<string> {
	const { style, familyName } = entry;
	const size = parsePixels(style.fontSize);
	if (!size.ok) return withErrorContext('fontSize', size);
	const lineHeight = parsePixels(style.lineHeight);
	if (!lineHeight.ok) return withErrorContext('lineHeight', lineHeight);
	const weight = toSwiftFontWeight(style.fontWeight);
	if (!weight.ok) return withErrorContext('fontWeight', weight);
	const letterSpacing =
		style.letterSpacing === undefined ? ok(0) : parseEm(style.letterSpacing);
	if (!letterSpacing.ok)
		return withErrorContext('letterSpacing', letterSpacing);
	const parameters = [
		`family: ${formatStringLiteral(familyName)}`,
		`size: ${formatNumber(size.value)}`,
		`lineHeight: ${formatNumber(lineHeight.value)}`,
		`weight: .${weight.value}`,
		`letterSpacingEm: ${formatNumber(letterSpacing.value)}`,
	];
	return ok(`TextStyle(${parameters.join(', ')})`);
}

function buildShadowStyleExpression(token: ShadowToken): Result<string> {
	const light = parseShadowList(token.value.light);
	if (!light.ok) return withErrorContext('light', light);
	const dark = parseShadowList(token.value.dark);
	if (!dark.ok) return withErrorContext('dark', dark);
	return ok(
		`ShadowStyle(light: ${formatShadowLayers(light.value)}, dark: ${formatShadowLayers(dark.value)})`,
	);
}

function formatShadowLayers(layers: readonly ShadowLayerValue[]): string {
	return `[${layers.map(formatShadowLayer).join(', ')}]`;
}

function formatShadowLayer(layer: ShadowLayerValue): string {
	const parameters = [
		`x: ${formatNumber(layer.x)}`,
		`y: ${formatNumber(layer.y)}`,
		`blur: ${formatNumber(layer.blur)}`,
		`spread: ${formatNumber(layer.spread)}`,
		`color: ${formatRgbaWithDecimalChannels(layer.color)}`,
	];
	return `ShadowLayer(${parameters.join(', ')})`;
}

function collectConstants<T>(
	label: string,
	items: readonly T[],
	convert: (item: T) => Result<SwiftConstant>,
): Result<SwiftConstant[]> {
	const constants = collectResults(items, convert);
	if (!constants.ok) return withErrorContext(label, constants);
	return ensureUniqueIdentifiers(label, constants.value);
}

function defineConstant(
	tokenName: string,
	usage: string,
	typeAnnotation: string | undefined,
	expression: Result<string>,
): Result<SwiftConstant> {
	const identifier = toSwiftIdentifier(tokenName);
	if (!identifier.ok) return identifier;
	if (!expression.ok) return withErrorContext(`"${tokenName}"`, expression);
	return ok({
		identifier: identifier.value,
		usage: usage.replace(/\s+/g, ' ').trim(),
		typeAnnotation,
		expression: expression.value,
	});
}

function ensureUniqueIdentifiers(
	label: string,
	constants: SwiftConstant[],
): Result<SwiftConstant[]> {
	const seen = new Set<string>();
	for (const { identifier } of constants) {
		if (seen.has(identifier)) {
			return err(`${label}: duplicate Swift identifier "${identifier}"`);
		}
		seen.add(identifier);
	}
	return ok(constants);
}
