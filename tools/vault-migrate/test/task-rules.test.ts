import { beforeAll, describe, expect, it } from 'vitest';
import type { ProvenanceHasher } from '../src/provenance-core.ts';
import {
	changeFor,
	copyFixtures,
	type FixtureWorkspace,
	HUB,
	loadHasher,
	planFixture,
	UNNUMBERED_STORY,
	WIDGET_STORY,
} from './helpers.ts';

let hasher: ProvenanceHasher;
let workspace: FixtureWorkspace;

beforeAll(async () => {
	hasher = await loadHasher();
	workspace = copyFixtures();
});

describe('task line rules', () => {
	it('appends [id:: task-N] before the first inline field, normalises [k::v] and [X]', () => {
		const { plan } = planFixture(workspace.work, 'mini-work', workspace, hasher);
		const after = changeFor(plan, WIDGET_STORY).after;
		expect(after).toContain(
			'- [x] Alignment from business on the palette  [id:: task-',
		);
		expect(after).toMatch(
			/\[id:: task-\d+\] \[created:: 2026-07-09\] \[assigned_to:: GBS\] \[completion:: 2026-10-01\]/,
		);
		expect(after).toContain('- [ ] Loose checkbox without fields [id:: task-');
		expect(after).not.toContain('[X]');
		const proseLines = after
			.split('\n')
			.filter((line) => !line.includes('inside a fence'));
		expect(proseLines.join('\n')).not.toMatch(/\[assigned_to::[A-Za-z]/);
		expect(after).toContain('\t- Nested note under the task, not a task');
	});

	it('never invents completion dates', () => {
		const { plan } = planFixture(workspace.work, 'mini-work', workspace, hasher);
		const after = changeFor(plan, WIDGET_STORY).after;
		expect(after.match(/\[completion::/g)?.length).toBe(1);
	});

	it('turns - [1] hub lines into plain bullets', () => {
		const { plan } = planFixture(workspace.work, 'mini-work', workspace, hasher);
		const after = changeFor(plan, HUB).after;
		expect(after).not.toContain('- [1]');
		expect(after).toContain(
			'- Alpha ecosystem adjusted to the current environment:',
		);
	});

	it('inserts ## Next Steps before ## Pending Input in stories, but not inside an AI unit', () => {
		const { plan } = planFixture(workspace.work, 'mini-work', workspace, hasher);
		const widget = changeFor(plan, WIDGET_STORY).after.split('\n');
		expect(widget.indexOf('## Next Steps')).toBe(
			widget.indexOf('## Pending Input') - 1,
		);
		expect(changeFor(plan, UNNUMBERED_STORY).after).not.toContain(
			'## Next Steps',
		);
		expect(
			plan.reports.some(
				(report) =>
					report.includes(UNNUMBERED_STORY) &&
					report.includes('Next Steps not inserted'),
			),
		).toBe(true);
	});

	it('leaves fenced code untouched', () => {
		const { plan } = planFixture(workspace.work, 'mini-work', workspace, hasher);
		const after = changeFor(plan, WIDGET_STORY).after;
		expect(after).toContain(
			'- [ ] a task inside a fence [assigned_to::GBS]\n[[Widget colour picker (new look)]]\n%% ai:begin claude-x · 2026-01-01 · 000000000000 %%',
		);
		const personal = planFixture(
			workspace.personal,
			'mini-personal',
			workspace,
			hasher,
		).plan;
		expect(
			changeFor(personal, 'Projects/Realname/Codename.md').after,
		).toContain(
			'- [ ] Confirm field mapping [id:: task-118] [created:: 2026-10-15]',
		);
	});

	it('gives no ids to template task lines and allocates ids in file order from the counter', () => {
		const { plan, allocator } = planFixture(
			workspace.work,
			'mini-work',
			workspace,
			hasher,
		);
		expect(changeFor(plan, '_templates/story-template.md').after).not.toContain(
			'[id::',
		);
		expect(plan.allocatedIds[0]).toBe('task-1');
		expect(allocator.peek('task')).toBe(plan.allocatedIds.length + 1);
	});
});
