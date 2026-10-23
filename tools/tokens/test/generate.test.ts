import path from 'node:path';
import { describe, expect, it } from 'vitest';
import {
	generateTokensSwiftFromFile,
	readTextFile,
} from '../src/generate-from-file.ts';
import {
	DEFAULT_SWIFT_OUTPUT_PATH,
	DEFAULT_TOKENS_PATH,
	REPOSITORY_ROOT,
} from '../src/paths.ts';
import { generateTokensSwift } from '../src/swift-file.ts';
import { formatDecimal, formatNumber } from '../src/swift-literals.ts';
import { parseTokenFile, type TokenFile } from '../src/token-schema.ts';

function minimalTokenFile(): TokenFile {
	const parsed = parseTokenFile({
		color: {
			themes: [{ id: 'light' }, { id: 'dark' }],
			tokens: [
				{
					name: 'surface-window',
					value: { light: '#e9ebee', dark: '#10161d' },
					usage: 'Backdrop.',
				},
			],
		},
		type: {
			families: { sans: '"Geist", system-ui, sans-serif' },
			groups: [
				{
					family: 'sans',
					styles: [
						{
							name: 'title-large',
							fontSize: '20px',
							lineHeight: '26px',
							fontWeight: 700,
							letterSpacing: '-0.01em',
							usage: 'Title.',
						},
					],
				},
			],
		},
		spacing: { tokens: [{ name: 'space-2', value: '2px', usage: 'Gap.' }] },
		radius: { tokens: [{ name: 'radius-xs', value: '4px', usage: 'Tag.' }] },
		size: { tokens: [{ name: 'size-icon', value: '16px', usage: 'Icon.' }] },
		shadow: {
			tokens: [
				{
					name: 'shadow-raised',
					value: {
						light: '0 1px 2px rgba(27,38,51,0.12)',
						dark: '0 1px 2px rgba(0,0,0,0.40)',
					},
					usage: 'Raised.',
				},
			],
		},
	});
	if (!parsed.ok) throw new Error(parsed.error);
	return parsed.value;
}

describe('generateTokensSwift', () => {
	it('renders each section with doc comments and Swift names', () => {
		const result = generateTokensSwift(minimalTokenFile());
		expect(result.ok).toBe(true);
		if (!result.ok) return;
		const swift = result.value;
		expect(swift).toContain(
			'\t\tstatic let surfaceWindow = Color(light: RGBA(0xe9, 0xeb, 0xee, 1.0), dark: RGBA(0x10, 0x16, 0x1d, 1.0))',
		);
		expect(swift).toContain('\t\t/// Gap.\n\t\tstatic let space2: CGFloat = 2');
		expect(swift).toContain(
			'static let titleLarge = TextStyle(family: "Geist", size: 20, lineHeight: 26, weight: .bold, letterSpacingEm: -0.01)',
		);
		expect(swift).toContain(
			'static let shadowRaised = ShadowStyle(light: [ShadowLayer(x: 0, y: 1, blur: 2, spread: 0, color: RGBA(27, 38, 51, 0.12))], dark: [ShadowLayer(x: 0, y: 1, blur: 2, spread: 0, color: RGBA(0, 0, 0, 0.4))])',
		);
		expect(swift.endsWith('}\n')).toBe(true);
	});

	it('renders motion durations in seconds when the file has them', () => {
		const result = generateTokensSwift({
			...minimalTokenFile(),
			motion: {
				tokens: [{ name: 'motion-lift', value: '180ms', usage: 'Lift.' }],
			},
		});
		expect(result.ok).toBe(true);
		if (!result.ok) return;
		expect(result.value).toContain(
			'\tenum Motion {\n\t\t/// Lift.\n\t\tstatic let motionLift: TimeInterval = 0.18',
		);
		const bad = generateTokensSwift({
			...minimalTokenFile(),
			motion: { tokens: [{ name: 'motion-x', value: '1s', usage: 'X.' }] },
		});
		expect(bad.ok).toBe(false);
	});

	it('fails with a Result for a token name that is not a valid Swift identifier', () => {
		const tokenFile = minimalTokenFile();
		const result = generateTokensSwift({
			...tokenFile,
			spacing: { tokens: [{ name: '2-up', value: '2px', usage: 'Bad.' }] },
		});
		expect(result.ok).toBe(false);
		if (!result.ok) expect(result.error).toContain('Spacing');
	});

	it('fails when two tokens map to the same identifier', () => {
		const tokenFile = minimalTokenFile();
		const result = generateTokensSwift({
			...tokenFile,
			radius: {
				tokens: [
					{ name: 'radius-xs', value: '4px', usage: 'A.' },
					{ name: 'radius-xs', value: '5px', usage: 'B.' },
				],
			},
		});
		expect(result.ok).toBe(false);
	});

	it('fails when a type group names an unknown family', () => {
		const tokenFile = minimalTokenFile();
		const result = generateTokensSwift({
			...tokenFile,
			type: {
				families: {},
				groups: tokenFile.type.groups,
			},
		});
		expect(result.ok).toBe(false);
	});
});

describe('parseTokenFile', () => {
	it('rejects a malformed file with a Result error instead of throwing', () => {
		expect(parseTokenFile({ color: { tokens: 'nope' } }).ok).toBe(false);
		expect(parseTokenFile(null).ok).toBe(false);
		expect(parseTokenFile([]).ok).toBe(false);
	});

	it('requires the light and dark themes', () => {
		const tokenFile = minimalTokenFile();
		const result = parseTokenFile({
			...tokenFile,
			color: { ...tokenFile.color, themes: [{ id: 'light' }] },
		});
		expect(result.ok).toBe(false);
	});
});

describe('Swift literal formatting', () => {
	it('formats numbers minimally and decimals with a fraction', () => {
		expect(formatNumber(2)).toBe('2');
		expect(formatNumber(-0.01)).toBe('-0.01');
		expect(formatDecimal(1)).toBe('1.0');
		expect(formatDecimal(0.051)).toBe('0.051');
	});

	it('throws for values that are not finite, a programmer error', () => {
		expect(() => formatNumber(Number.NaN)).toThrow();
	});
});

describe('committed Tokens.swift', () => {
	it('equals what design/tokens.json generates (same as the --check script)', () => {
		const generated = generateTokensSwiftFromFile(
			path.join(REPOSITORY_ROOT, DEFAULT_TOKENS_PATH),
		);
		const committed = readTextFile(
			path.join(REPOSITORY_ROOT, DEFAULT_SWIFT_OUTPUT_PATH),
		);
		expect(generated.ok).toBe(true);
		expect(committed.ok).toBe(true);
		if (generated.ok && committed.ok) {
			expect(committed.value === generated.value).toBe(true);
		}
	});

	it('reports a missing input file as a Result error', () => {
		expect(generateTokensSwiftFromFile('/nonexistent/tokens.json').ok).toBe(
			false,
		);
	});
});
