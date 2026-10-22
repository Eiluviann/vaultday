import { existsSync, readdirSync } from 'node:fs';
import path from 'node:path';
import { beforeAll, describe, expect, it } from 'vitest';
import { applyPlan } from '../src/apply.ts';
import { IdAllocator } from '../src/id-allocator.ts';
import { isPlanEmpty } from '../src/migration.ts';
import type { ProvenanceHasher } from '../src/provenance-core.ts';
import { renderPlan } from '../src/render-plan.ts';
import { buildTickPlan } from '../src/tick.ts';
import { verifyLinks } from '../src/verify-links.ts';
import {
	copyFixtures,
	type FixtureWorkspace,
	loadHasher,
	planFixture,
	readVaultFile,
} from './helpers.ts';

let hasher: ProvenanceHasher;
let workspace: FixtureWorkspace;

beforeAll(async () => {
	hasher = await loadHasher();
	workspace = copyFixtures();
});

describe('apply, idempotence, links and tick', () => {
	it('applies Work then Personal, writes manifests, and a second run plans nothing', () => {
		expect(verifyLinks(workspace.work)).toEqual([]);
		const work = planFixture(workspace.work, 'mini-work', workspace, hasher);
		const appliedWork = applyPlan(work.plan, {
			stateDirectory: workspace.stateDirectory,
			allocator: work.allocator,
		});
		expect(appliedWork.ok).toBe(true);
		const personal = planFixture(
			workspace.personal,
			'mini-personal',
			workspace,
			hasher,
		);
		const appliedPersonal = applyPlan(personal.plan, {
			stateDirectory: workspace.stateDirectory,
			allocator: personal.allocator,
		});
		expect(appliedPersonal.ok).toBe(true);

		expect(
			existsSync(
				path.join(
					workspace.work,
					'Projects/Alpha/STRY0000001 Widget colour picker/STRY0000001 Widget colour picker.md',
				),
			),
		).toBe(true);
		expect(
			existsSync(
				path.join(
					workspace.work,
					'Projects/Alpha/STRY0000001 Widget colour picker/_agent/exploration/export.txt',
				),
			),
		).toBe(true);
		expect(
			existsSync(
				path.join(
					workspace.work,
					'Projects/Alpha/Widget colour picker (new look)',
				),
			),
		).toBe(false);
		expect(
			existsSync(path.join(workspace.work, 'Reference/Test users.md')),
		).toBe(true);
		expect(existsSync(path.join(workspace.work, '.trash/Untitled.md'))).toBe(
			true,
		);
		expect(existsSync(path.join(workspace.work, 'Untitled.md'))).toBe(false);
		expect(
			existsSync(path.join(workspace.work, 'Projects/Alpha/Unnumbered draft')),
		).toBe(false);
		expect(
			existsSync(
				path.join(
					workspace.personal,
					'Projects/Realname/_agent/prompts/unnumbered-draft.md',
				),
			),
		).toBe(true);
		expect(
			existsSync(
				path.join(workspace.personal, 'Projects/Realname/Realname.md'),
			),
		).toBe(true);
		expect(
			existsSync(
				path.join(
					workspace.personal,
					'Projects/Realname/_agent/research/report/raw.md',
				),
			),
		).toBe(true);
		expect(
			existsSync(
				path.join(workspace.personal, 'Projects/Realname/research_notes'),
			),
		).toBe(false);
		expect(
			existsSync(path.join(workspace.personal, 'Projects/Realname/reports')),
		).toBe(false);
		expect(existsSync(path.join(workspace.personal, 'Log'))).toBe(true);
		expect(existsSync(path.join(workspace.personal, 'Daily'))).toBe(false);
		expect(
			readdirSync(workspace.stateDirectory).filter((name) =>
				name.includes('apply-manifest'),
			).length,
		).toBe(2);
		expect(readVaultFile(workspace.personal, 'Home.md')).toContain('`Daily/`');

		const secondWork = planFixture(workspace.work, 'mini-work', workspace, hasher);
		const secondPersonal = planFixture(
			workspace.personal,
			'mini-personal',
			workspace,
			hasher,
		);
		expect(isPlanEmpty(secondWork.plan)).toBe(true);
		expect(isPlanEmpty(secondPersonal.plan)).toBe(true);
		expect(secondWork.allocator.peek('task')).toBe(work.allocator.peek('task'));
	});

	it('all links resolve after the renames and moves (only the note moved to the other vault is unresolved)', () => {
		expect(verifyLinks(workspace.work).map((link) => link.target)).toEqual([
			'Unnumbered draft',
		]);
		expect(verifyLinks(workspace.personal)).toEqual([]);
		expect(readVaultFile(workspace.work, 'Projects/Alpha/Alpha.md')).toContain(
			'[[STRY0000002 A very long story title that certainly|Long one]]',
		);
		expect(
			readVaultFile(workspace.personal, 'Projects/Realname/Kickoff.md'),
		).toContain('[[Realname]]');
	});

	it('--tick round trip changes exactly one line each way', () => {
		const forward = buildTickPlan(workspace.work, 'task-7', '2026-10-22');
		expect(forward.ok).toBe(true);
		if (!forward.ok) return;
		const change = forward.value.changes[0];
		expect(change).toBeDefined();
		if (change === undefined) return;
		const beforeLines = change.before.split('\n');
		const afterLines = change.after.split('\n');
		expect(afterLines.length).toBe(beforeLines.length);
		const differing = beforeLines
			.map((line, index) => (line === afterLines[index] ? -1 : index))
			.filter((index) => index !== -1);
		expect(differing.length).toBe(1);
		expect(afterLines[differing[0] ?? 0]).toMatch(
			/^- \[x\] .*\[id:: task-7\].*\[completion:: 2026-10-22\]$/,
		);
		const applied = applyPlan(forward.value, {
			stateDirectory: workspace.stateDirectory,
			allocator: new IdAllocator(workspace.work, workspace.idStateDirectory),
		});
		expect(applied.ok).toBe(true);

		const backward = buildTickPlan(workspace.work, 'task-7', '2026-10-22');
		expect(backward.ok).toBe(true);
		if (!backward.ok) return;
		expect(backward.value.changes[0]?.after).toBe(change.before);
		expect(buildTickPlan(workspace.work, 'task-999', '2026-10-22').ok).toBe(
			false,
		);
	});

	it('renders nothing for an empty plan and a summary otherwise', () => {
		const second = planFixture(workspace.work, 'mini-work', workspace, hasher);
		expect(isPlanEmpty(second.plan)).toBe(true);
		const fresh = copyFixtures();
		const first = planFixture(fresh.work, 'mini-work', fresh, hasher);
		expect(renderPlan(first.plan)).toContain('## Renames');
	});
});
