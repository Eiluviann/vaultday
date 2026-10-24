import { reportNoteText } from '@vaultday/provenance-core';
import { describe, expect, it } from 'vitest';
import {
	refreshProvenanceHashes,
	settleTasksAfter,
	shiftDates,
} from '../src/vault-text.ts';

describe('shiftDates', () => {
	it('moves every real date and leaves look-alikes', () => {
		expect(shiftDates('due 2026-10-30, id 2026-99-99', 7)).toBe(
			'due 2026-11-06, id 2026-99-99',
		);
	});
});

describe('settleTasksAfter', () => {
	it('reopens a task completed after today and keeps earlier ones done', () => {
		const text = [
			'- [x] Later  [id:: task-1] [created:: 2026-10-19] [completion:: 2026-10-23]',
			'\t- [x] Earlier  [id:: task-2] [created:: 2026-10-19] [completion:: 2026-10-20]',
		].join('\n');
		expect(settleTasksAfter(text, '2026-10-21')).toBe(
			[
				'- [ ] Later  [id:: task-1] [created:: 2026-10-19]',
				'\t- [x] Earlier  [id:: task-2] [created:: 2026-10-19] [completion:: 2026-10-20]',
			].join('\n'),
		);
	});

	it('moves a creation date after today to today', () => {
		expect(
			settleTasksAfter(
				'- [ ] Soon  [id:: task-3] [created:: 2026-10-23]',
				'2026-10-21',
			),
		).toBe('- [ ] Soon  [id:: task-3] [created:: 2026-10-21]');
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
});
