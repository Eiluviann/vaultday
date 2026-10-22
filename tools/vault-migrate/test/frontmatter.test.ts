import { describe, expect, it } from 'vitest';
import { Frontmatter } from '../src/frontmatter.ts';

describe('Frontmatter', () => {
	it('keeps untouched entries byte-identical, including Templater expressions and block lists', () => {
		const lines = [
			'created: <% tp.date.now("YYYY-MM-DD") %>',
			'tags:',
			'  - project',
			'  - llm',
			'related: ["[[A]]", "[[B]]"]',
		];
		const frontmatter = Frontmatter.parse(lines);
		frontmatter.setScalar('type', 'project', 'start');
		expect(frontmatter.render()).toEqual(['type: project', ...lines]);
		expect(frontmatter.list('tags')).toEqual(['project', 'llm']);
		expect(frontmatter.list('related')).toEqual(['[[A]]', '[[B]]']);
	});

	it('renames keys in place and places new keys relative to others', () => {
		const frontmatter = Frontmatter.parse([
			'Number: STRY1',
			'Status: On Hold',
			'sort_index: 10',
		]);
		frontmatter.rename('Number', 'number');
		frontmatter.setScalar('ticket', '', { after: 'number' });
		frontmatter.setScalar('status', 'on-hold', 'end');
		frontmatter.rename('Status', 'old_status');
		frontmatter.remove('old_status');
		expect(frontmatter.render()).toEqual([
			'number: STRY1',
			'ticket:',
			'sort_index: 10',
			'status: on-hold',
		]);
	});

	it('quotes list items with spaces and scalars with special characters', () => {
		const frontmatter = Frontmatter.empty();
		frontmatter.setList('aliases', ['STRY1', 'A&F form update']);
		frontmatter.setScalar('note_system', '1.0');
		frontmatter.setScalar('title', 'Say "hi": now');
		expect(frontmatter.render()).toEqual([
			'aliases: [STRY1, "A&F form update"]',
			'note_system: 1.0',
			'title: "Say \\"hi\\": now"',
		]);
	});
});
