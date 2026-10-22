import { describe, expect, it } from 'vitest';
import { planRenames } from '../src/rename-plan.ts';
import {
	SHORT_TITLE_OVERRIDES,
	shortenTitle,
	storyFolderName,
} from '../src/rules/short-titles.ts';

describe('short titles', () => {
	it('uses the override table for the eight agreed stories', () => {
		expect(
			storyFolderName(
				'STRY0681010',
				'CSAT integrated with Portfolio hierarchy',
			),
		).toBe('STRY0681010 CSAT Portfolio hierarchy');
		expect(
			storyFolderName(
				'STRY0602768',
				'CSAT rating system updated (from 5 to 2)',
			),
		).toBe('STRY0602768 CSAT rating 5 to 2');
		expect(
			storyFolderName(
				'STRY0602776',
				'Open Feedback integrated with Portfolio hierarchy',
			),
		).toBe('STRY0602776 Open Feedback Portfolio hierarchy');
		expect(
			storyFolderName(
				'STRY0602779',
				'Open Feedback form updated according to the Portfolio hierarchy and new categories',
			),
		).toBe('STRY0602779 Open Feedback form new categories');
		expect(
			storyFolderName(
				'STRY0602785',
				'Issue Log App integrated with Portfolio hierarchy (including update of existing categories)',
			),
		).toBe('STRY0602785 Issue Log Portfolio hierarchy');
		expect(
			storyFolderName(
				'STRY0602788',
				'Issue Log cases assigned without human intervention',
			),
		).toBe('STRY0602788 Issue Log auto-assignment');
		expect(
			storyFolderName(
				'STRY0602796',
				'Issue Log cases logged on behalf available under My Issues (Portal)',
			),
		).toBe('STRY0602796 Issue Log on-behalf in My Issues');
		expect(
			storyFolderName(
				'STRY0602806',
				'Issue Log cases integrated with operational Workspaces',
			),
		).toBe('STRY0602806 Issue Log in Workspaces');
		expect(SHORT_TITLE_OVERRIDES.size).toBe(8);
	});

	it('keeps unchanged titles for STRY0682156 and STRY0679619', () => {
		expect(
			storyFolderName('STRY0682156', 'A&F Ask A Question form update'),
		).toBe('STRY0682156 A&F Ask A Question form update');
		expect(
			storyFolderName('STRY0679619', 'Finance notification field audit'),
		).toBe('STRY0679619 Finance notification field audit');
	});

	it('strips parentheticals and cuts at a word boundary within 40 characters', () => {
		expect(shortenTitle('Widget colour picker (new look)')).toBe(
			'Widget colour picker',
		);
		const long = shortenTitle(
			'A very long story title that certainly exceeds forty characters (details)',
		);
		expect(long.length).toBeLessThanOrEqual(40);
		expect(long).toBe('A very long story title that certainly');
	});
});

describe('planRenames', () => {
	it('renames numbered folder notes, skips unnumbered ones and already-prefixed ones', () => {
		const result = planRenames([
			{
				relativePath: 'Projects/P/Old title/Old title.md',
				number: 'STRY0000009',
			},
			{ relativePath: 'Projects/P/Draft/Draft.md', number: '' },
			{
				relativePath: 'Projects/P/STRY0000010 Done/STRY0000010 Done.md',
				number: 'STRY0000010',
			},
			{ relativePath: 'Projects/P/Loose note.md', number: 'STRY0000011' },
		]);
		expect(result.ok).toBe(true);
		if (!result.ok) return;
		expect(result.value).toEqual([
			{
				number: 'STRY0000009',
				oldTitle: 'Old title',
				newTitle: 'STRY0000009 Old title',
				fromDirectory: 'Projects/P/Old title',
				toDirectory: 'Projects/P/STRY0000009 Old title',
				fromNote: 'Projects/P/Old title/Old title.md',
				toNote: 'Projects/P/STRY0000009 Old title/STRY0000009 Old title.md',
			},
		]);
	});

	it('refuses two stories that would collide', () => {
		const result = planRenames([
			{ relativePath: 'Projects/P/A (x)/A (x).md', number: 'STRY1' },
			{ relativePath: 'Projects/P/A (y)/A (y).md', number: 'STRY1' },
		]);
		expect(result.ok).toBe(false);
	});
});
