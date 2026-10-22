import { beforeAll, describe, expect, it } from 'vitest';
import type { ProvenanceHasher } from '../src/provenance-core.ts';
import {
	countBumpedNotes,
	countLegacyMarkedNotes,
	renderSummary,
} from '../src/render-plan.ts';
import {
	changeFor,
	copyFixtures,
	type FixtureWorkspace,
	frontmatterOf,
	HUB,
	LONG_STORY,
	loadHasher,
	MIGRATION_DATE,
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

describe('story frontmatter rules', () => {
	it('renames Number/Ticket/Status, adds type story, derives source_* and leaves source_id empty', () => {
		const { plan } = planFixture(workspace.work, 'mini-work', workspace, hasher);
		const frontmatter = frontmatterOf(changeFor(plan, WIDGET_STORY).after);
		expect(frontmatter[0]).toBe('type: story');
		expect(frontmatter).toContain('number: STRY0000001');
		expect(frontmatter.find((line) => line.startsWith('ticket: '))).toContain(
			'nav_to.do',
		);
		expect(frontmatter).toContain('source_host: example.service-now.com');
		expect(frontmatter).toContain('source_kind: rm_story');
		expect(frontmatter).toContain('source_id:');
		expect(
			frontmatter.some((line) =>
				/^(Number|Ticket|Status|note-system-ver):/.test(line),
			),
		).toBe(false);
		expect(
			plan.reports.some(
				(report) =>
					report.includes(WIDGET_STORY) &&
					report.includes('source_id left empty'),
			),
		).toBe(true);
	});

	it('lowercases and hyphenates status values from the table', () => {
		const { plan } = planFixture(workspace.work, 'mini-work', workspace, hasher);
		expect(frontmatterOf(changeFor(plan, WIDGET_STORY).after)).toContain(
			'status: in-progress',
		);
		expect(frontmatterOf(changeFor(plan, LONG_STORY).after)).toContain(
			'status: on-hold',
		);
		expect(frontmatterOf(changeFor(plan, UNNUMBERED_STORY).after)).toContain(
			'status: draft',
		);
	});

	it('adds empty priority/due/estimate, created from the birthtime manifest and note_system 1.0', () => {
		const { plan } = planFixture(workspace.work, 'mini-work', workspace, hasher);
		const frontmatter = frontmatterOf(changeFor(plan, WIDGET_STORY).after);
		expect(frontmatter).toContain('priority:');
		expect(frontmatter).toContain('due:');
		expect(frontmatter).toContain('estimate:');
		expect(frontmatter).toContain('created: 2025-10-09');
		expect(frontmatter).toContain('note_system: 1.0');
		expect(frontmatter).toContain('sort_index: 1000');
	});

	it('aliases carry the number and the old title when the story is renamed; unnumbered stories get none', () => {
		const { plan } = planFixture(workspace.work, 'mini-work', workspace, hasher);
		expect(frontmatterOf(changeFor(plan, WIDGET_STORY).after)).toContain(
			'aliases: [STRY0000001, "Widget colour picker (new look)"]',
		);
		expect(
			frontmatterOf(changeFor(plan, UNNUMBERED_STORY).after).some((line) =>
				line.startsWith('aliases'),
			),
		).toBe(false);
	});

	it('story templates get the key renames and catalogue keys but no created or aliases', () => {
		const { plan } = planFixture(workspace.work, 'mini-work', workspace, hasher);
		const frontmatter = frontmatterOf(
			changeFor(plan, '_templates/story-template.md').after,
		);
		expect(frontmatter).toEqual([
			'type: story',
			'number:',
			'ticket:',
			'source_host:',
			'source_kind:',
			'source_id:',
			'status: draft',
			'priority:',
			'due:',
			'estimate:',
			'note_system: 1.0',
		]);
	});
});

describe('note_system bump', () => {
	it('every touched note in both fixtures ends with note_system: 1.0 and no note-system-ver, and the summary counts them', () => {
		for (const [vault, profile] of [
			[workspace.work, 'mini-work'],
			[workspace.personal, 'mini-personal'],
		] as const) {
			const { plan } = planFixture(vault, profile, workspace, hasher);
			for (const change of plan.changes) {
				expect(frontmatterOf(change.after), change.fromPath).toContain(
					'note_system: 1.0',
				);
				expect(change.after, change.fromPath).not.toContain('note-system-ver');
			}
			expect(countBumpedNotes(plan)).toBe(plan.changes.length);
			expect(renderSummary(plan)).toContain(
				`notes bumped to note_system: 1.0: ${plan.changes.length}`,
			);
		}
	});
});

describe('legacy marking', () => {
	it('adds legacy: true and migrated: <date> right after note_system on every touched note except templates', () => {
		for (const [vault, profile] of [
			[workspace.work, 'mini-work'],
			[workspace.personal, 'mini-personal'],
		] as const) {
			const { plan } = planFixture(vault, profile, workspace, hasher);
			const notes = plan.changes.filter(
				(change) => change.noteClass !== 'template',
			);
			for (const change of notes) {
				const frontmatter = frontmatterOf(change.after);
				const versionIndex = frontmatter.indexOf('note_system: 1.0');
				expect(
					frontmatter.slice(versionIndex, versionIndex + 3),
					change.fromPath,
				).toEqual([
					'note_system: 1.0',
					'legacy: true',
					`migrated: ${MIGRATION_DATE}`,
				]);
			}
			if (profile === 'mini-work')
				expect(
					frontmatterOf(changeFor(plan, '_templates/story-template.md').after),
				).not.toContain('legacy: true');
			expect(countLegacyMarkedNotes(plan)).toBe(notes.length);
			expect(renderSummary(plan)).toContain(
				`notes marked legacy: true / migrated: ${notes.length}`,
			);
		}
	});
});

describe('hub and other-note rules', () => {
	it('Work hubs get type project, key, status active and toggl_project', () => {
		const { plan } = planFixture(workspace.work, 'mini-work', workspace, hasher);
		const frontmatter = frontmatterOf(changeFor(plan, HUB).after);
		expect(frontmatter.slice(0, 4)).toEqual([
			'type: project',
			'key: ALPHA',
			'status: active',
			'toggl_project: Alpha Co',
		]);
		expect(frontmatter).toContain('created: 2025-06-15');
		expect(plan.hubs).toEqual([{ path: HUB, key: 'ALPHA' }]);
	});

	it('applies the type-guess table (type, kind, status, story) and reports notes without a rule', () => {
		const { plan } = planFixture(workspace.work, 'mini-work', workspace, hasher);
		const frontmatter = frontmatterOf(
			changeFor(
				plan,
				'Projects/Alpha/Widget colour picker (new look)/Current widget.md',
			).after,
		);
		expect(frontmatter).toContain('type: analysis');
		expect(frontmatter).toContain('kind: audit');
		expect(frontmatter).toContain('status: final');
		expect(frontmatter).toContain('story: STRY0000001');
		expect(plan.typeGuesses.map((guess) => guess.path)).toContain(
			'Test users.md',
		);
	});

	it('Personal project hubs get type project and keep kind and tags; type guesses may replace an off-catalogue type', () => {
		const { plan } = planFixture(
			workspace.personal,
			'mini-personal',
			workspace,
			hasher,
		);
		const hobby = frontmatterOf(
			changeFor(plan, 'Projects/Hobby/Hobby.md').after,
		);
		expect(hobby[0]).toBe('type: project');
		expect(hobby).toContain('kind: hobby');
		expect(hobby).toContain('  - project');
		const kickoff = frontmatterOf(
			changeFor(plan, 'Projects/Realname/Kickoff.md').after,
		);
		expect(kickoff).toContain('type: note');
		expect(kickoff).toContain('kind: kickoff');
		expect(kickoff).toContain('project: "[[Realname]]"');
		const codename = frontmatterOf(
			changeFor(plan, 'Projects/Realname/Codename.md').after,
		);
		expect(codename).toContain('aliases: [Codename]');
	});

	it('creates a frontmatter block for notes without one, above the ai:note marker', () => {
		const { plan } = planFixture(
			workspace.personal,
			'mini-personal',
			workspace,
			hasher,
		);
		const after = changeFor(
			plan,
			'Projects/Realname/reports/Research report.md',
		).after.split('\n');
		expect(after[0]).toBe('---');
		expect(after.slice(1, after.indexOf('---', 1))).toContain('type: research');
		expect(after[after.indexOf('---', 1) + 1]).toMatch(/^%% ai:note /);
	});
});
