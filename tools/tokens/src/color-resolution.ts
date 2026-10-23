import type { Rgba } from './css-values.ts';
import { parseColorLiteral } from './css-values.ts';
import {
	collectResults,
	err,
	ok,
	type Result,
	withErrorContext,
} from './result.ts';
import type { ColorToken, ThemeId } from './token-schema.ts';

export type ResolvedColor = {
	name: string;
	usage: string;
	light: Rgba;
	dark: Rgba;
};

type TokensByName = ReadonlyMap<string, ColorToken>;

const ALIAS_PATTERN = /^\{([^{}]+)\}$/;

/** Resolves every color token to concrete light and dark values, following `{alias}` references. */
export function resolveColorTokens(
	tokens: readonly ColorToken[],
): Result<ResolvedColor[]> {
	const tokensByName = new Map(tokens.map((token) => [token.name, token]));
	return collectResults(tokens, (token) =>
		withErrorContext(
			`color "${token.name}"`,
			resolveColorToken(token, tokensByName),
		),
	);
}

function resolveColorToken(
	token: ColorToken,
	tokensByName: TokensByName,
): Result<ResolvedColor> {
	const light = resolveColorForTheme(token, 'light', tokensByName, []);
	if (!light.ok) return withErrorContext('light', light);
	const dark = resolveColorForTheme(token, 'dark', tokensByName, []);
	if (!dark.ok) return withErrorContext('dark', dark);
	return ok({
		name: token.name,
		usage: token.usage,
		light: light.value,
		dark: dark.value,
	});
}

function resolveColorForTheme(
	token: ColorToken,
	theme: ThemeId,
	tokensByName: TokensByName,
	aliasTrail: readonly string[],
): Result<Rgba> {
	const trail = [...aliasTrail, token.name];
	const rawValue = readThemeValue(token, theme);
	const aliasTarget = ALIAS_PATTERN.exec(rawValue.trim())?.[1];
	if (aliasTarget === undefined) return parseColorLiteral(rawValue);
	if (trail.includes(aliasTarget)) {
		return err(`alias cycle ${[...trail, aliasTarget].join(' -> ')}`);
	}
	const target = tokensByName.get(aliasTarget);
	if (target === undefined)
		return err(`alias to missing token "${aliasTarget}"`);
	return resolveColorForTheme(target, theme, tokensByName, trail);
}

function readThemeValue(token: ColorToken, theme: ThemeId): string {
	return typeof token.value === 'string' ? token.value : token.value[theme];
}
