import { reportNoteText } from '@vaultday/provenance-core';
import { describe, expect, it } from 'vitest';
import {
	holdNoteToCutoff,
	type NoteCutoff,
	refreshProvenanceHashes,
	shiftDates,
} from '../src/vault-text.ts';

const CUTOFF: NoteCutoff = {
	today: '2026-10-21',
	now: '12:00',
	doneTimes: new Map([
		['task-4', { date: '2026-10-21', time: '09:00' }],
		['task-5', { date: '2026-10-21', time: '15:00' }],
	]),
};

describe('shiftDates', () => {
	it('moves every real date and leaves look-alikes', () => {
		expect(shiftDates('due 2026-10-30, id 2026-99-99', 7)).toBe(
			'due 2026-11-06, id 2026-99-99',
		);
	});
});

describe('holdNoteToCutoff', () => {
	it('reopens a task completed after today and keeps earlier ones done', () => {
		const text = [
			'- [x] Later  [id:: task-1] [created:: 2026-10-19] [completion:: 2026-10-23]',
			'\t- [x] Earlier  [id:: task-2] [created:: 2026-10-19] [completion:: 2026-10-20]',
		].join('\n');
		expect(holdNoteToCutoff(text, CUTOFF)).toBe(
			[
				'- [ ] Later  [id:: task-1] [created:: 2026-10-19]',
				'\t- [x] Earlier  [id:: task-2] [created:: 2026-10-19] [completion:: 2026-10-20]',
			].join('\n'),
		);
	});

	it('keeps a task done earlier today and reopens one whose done line comes after now', () => {
		const text = [
			'- [x] Morning  [id:: task-4] [created:: 2026-10-19] [completion:: 2026-10-21]',
			'- [x] Afternoon  [id:: task-5] [created:: 2026-10-19] [completion:: 2026-10-21]',
			'- [x] No log line  [id:: task-6] [created:: 2026-10-19] [completion:: 2026-10-21]',
		].join('\n');
		expect(
			holdNoteToCutoff(text, CUTOFF)
				.split('\n')
				.map((line) => line.slice(0, 5)),
		).toEqual(['- [x]', '- [ ]', '- [x]']);
	});

	it('moves a creation date after today to today', () => {
		expect(
			holdNoteToCutoff(
				'- [ ] Soon  [id:: task-3] [created:: 2026-10-23]',
				CUTOFF,
			),
		).toBe('- [ ] Soon  [id:: task-3] [created:: 2026-10-21]');
	});

	it('drops a dated note line after today and dates a later marker today', () => {
		const text = [
			'## Notes',
			'- 2026-10-20 Decided yesterday.',
			'- 2026-10-23 Decided on Friday.',
			'%% ai:begin fable · 2026-10-22 · 0123456789ab %%',
		].join('\n');
		expect(holdNoteToCutoff(text, CUTOFF)).toBe(
			[
				'## Notes',
				'- 2026-10-20 Decided yesterday.',
				'%% ai:begin fable · 2026-10-21 · 0123456789ab %%',
			].join('\n'),
		);
	});
});

describe('refreshProvenanceHashes', () => {
	it('leaves every block and note marker intact, nested blocks included', () => {
		const text = [
			'%% ai:note fable · 2026-10-19 · 000000000000 %%',
			'# Research',
			'',
			'%% ai:begin fable · 2026-10-20 · 111111111111 %%',
			'Shifted text.',
			'%% ai:end %%',
			'',
		].join('\n');
		const refreshed = refreshProvenanceHashes('Research.md', text);
		expect(
			reportNoteText('Research.md', refreshed).units.map((unit) => unit.status),
		).toEqual(['intact', 'intact']);
		expect(refreshProvenanceHashes('Research.md', refreshed)).toBe(refreshed);
	});

	it('refreshes a marker in a note with Windows line endings', () => {
		const text = [
			'# Note',
			'%% ai:begin fable · 2026-10-20 · 111111111111 %%',
			'Shifted text.',
			'%% ai:end %%',
			'',
		].join('\r\n');
		const refreshed = refreshProvenanceHashes('Note.md', text);
		expect(refreshed).toContain('\r\n');
		expect(reportNoteText('Note.md', refreshed).units[0]?.status).toBe(
			'intact',
		);
	});

	it('rewrites the author hash of a verified marker and keeps the verification as written', () => {
		const text = [
			'# Note',
			'%% ai:begin fable · 2026-10-20 · 111111111111 · verified 2026-10-21 · 222222222222 %%',
			'Shifted text.',
			'%% ai:end %%',
			'',
		].join('\n');
		const marker =
			refreshProvenanceHashes('Note.md', text).split('\n')[1] ?? '';
		expect(marker).toMatch(
			/^%% ai:begin fable · 2026-10-20 · [0-9a-f]{12} · verified 2026-10-21 · 222222222222 %%$/,
		);
		expect(marker).not.toContain('111111111111');
		expect(
			reportNoteText('Note.md', refreshProvenanceHashes('Note.md', text))
				.units[0]?.status,
		).toBe('intact');
	});
});
