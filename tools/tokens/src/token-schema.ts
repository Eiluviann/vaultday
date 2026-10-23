import { z } from 'zod';
import { err, ok, type Result } from './result.ts';

export const THEME_IDS = ['light', 'dark'] as const;

export type ThemeId = (typeof THEME_IDS)[number];

const ThemedValueSchema = z.object({
	light: z.string(),
	dark: z.string(),
});

const ColorTokenSchema = z.object({
	name: z.string().min(1),
	value: z.union([z.string(), ThemedValueSchema]),
	usage: z.string(),
});

const LengthTokenSchema = z.object({
	name: z.string().min(1),
	value: z.string(),
	usage: z.string(),
});

const ShadowTokenSchema = z.object({
	name: z.string().min(1),
	value: ThemedValueSchema,
	usage: z.string(),
});

const TypeStyleSchema = z.object({
	name: z.string().min(1),
	fontSize: z.string(),
	lineHeight: z.string(),
	fontWeight: z.number(),
	letterSpacing: z.string().optional(),
	usage: z.string(),
});

const TypeGroupSchema = z.object({
	family: z.string(),
	styles: z.array(TypeStyleSchema),
});

const ThemesSchema = z
	.array(z.object({ id: z.string() }))
	.refine(
		(themes) =>
			THEME_IDS.every((themeId) =>
				themes.some((theme) => theme.id === themeId),
			),
		{ message: `must declare the themes ${THEME_IDS.join(' and ')}` },
	);

const TokenFileSchema = z.object({
	color: z.object({
		themes: ThemesSchema,
		tokens: z.array(ColorTokenSchema),
	}),
	type: z.object({
		families: z.record(z.string(), z.string()),
		groups: z.array(TypeGroupSchema),
	}),
	spacing: z.object({ tokens: z.array(LengthTokenSchema) }),
	radius: z.object({ tokens: z.array(LengthTokenSchema) }),
	size: z.object({ tokens: z.array(LengthTokenSchema) }),
	shadow: z.object({ tokens: z.array(ShadowTokenSchema) }),
});

export type ColorToken = z.infer<typeof ColorTokenSchema>;
export type LengthToken = z.infer<typeof LengthTokenSchema>;
export type ShadowToken = z.infer<typeof ShadowTokenSchema>;
export type TypeStyle = z.infer<typeof TypeStyleSchema>;
export type TokenFile = z.infer<typeof TokenFileSchema>;

/** Validates untrusted JSON against the token file shape. */
export function parseTokenFile(json: unknown): Result<TokenFile> {
	const parsed = TokenFileSchema.safeParse(json);
	if (parsed.success) return ok(parsed.data);
	const problems = parsed.error.issues.map(
		(issue) => `${issue.path.join('.') || '(root)'}: ${issue.message}`,
	);
	return err(`invalid token file:\n  ${problems.join('\n  ')}`);
}
