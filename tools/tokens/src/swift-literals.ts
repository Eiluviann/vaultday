import type { Rgba } from './css-values.ts';

/** Formats a finite number minimally (`2`, `-0.01`). */
export function formatNumber(value: number): string {
	const text = String(value);
	if (!Number.isFinite(value) || text.includes('e')) {
		throw new Error(`cannot format ${text} as a Swift literal`);
	}
	return text;
}

/** Formats a number as a Swift floating-point literal (`1.0`, `0.051`). */
export function formatDecimal(value: number): string {
	const text = formatNumber(value);
	return Number.isInteger(value) ? `${text}.0` : text;
}

/** Formats an `RGBA` initializer with hexadecimal channels. */
export function formatRgbaWithHexChannels(color: Rgba): string {
	const channels = [color.red, color.green, color.blue].map(formatHexByte);
	return `RGBA(${channels.join(', ')}, ${formatDecimal(color.alpha)})`;
}

/** Formats an `RGBA` initializer with decimal channels. */
export function formatRgbaWithDecimalChannels(color: Rgba): string {
	const channels = [color.red, color.green, color.blue].map(formatNumber);
	return `RGBA(${channels.join(', ')}, ${formatDecimal(color.alpha)})`;
}

/** Formats a Swift string literal. */
export function formatStringLiteral(text: string): string {
	return `"${text.replaceAll('\\', '\\\\').replaceAll('"', '\\"')}"`;
}

function formatHexByte(value: number): string {
	return `0x${value.toString(16).padStart(2, '0')}`;
}
