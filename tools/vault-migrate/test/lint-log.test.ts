import path from 'node:path';
import { describe, expect, it } from 'vitest';
import { lintLogs } from '../src/lint-log.ts';
import { FIXTURES_ROOT } from './helpers.ts';

const findings = lintLogs(path.join(FIXTURES_ROOT, 'mini-log'));
const rulesAt = (relativePath: string, line: number) =>
	findings
		.filter(
			(finding) =>
				finding.relativePath === relativePath && finding.line === line,
		)
		.map((finding) => finding.rule);
const BAD = 'Log/2026-10-23.md';

describe('lint-log', () => {
	it('accepts a well-formed day log and ignores lines outside ## Log', () => {
		expect(
			findings.filter(
				(finding) => finding.relativePath === 'Log/2026-10-22.md',
			),
		).toEqual([]);
	});

	it('checks the file name and the frontmatter', () => {
		expect(rulesAt('Log/notes.md', 1)).toEqual(['file-name']);
		expect(rulesAt(BAD, 1)).toEqual(['frontmatter-type', 'frontmatter-date']);
		expect(rulesAt('Log/2026-10-24.md', 5)).toEqual(['log-section']);
	});

	it('checks the time prefix, kind, references, task, agent, pipes, spacing and nesting', () => {
		expect(rulesAt(BAD, 6)).toEqual(['time-prefix']);
		expect(rulesAt(BAD, 7)).toEqual(['kind']);
		expect(rulesAt(BAD, 8)).toEqual(['kind']);
		expect(rulesAt(BAD, 9)).toEqual(['kind']);
		expect(rulesAt(BAD, 10)).toEqual(['reference']);
		expect(rulesAt(BAD, 11)).toEqual(['story']);
		expect(rulesAt(BAD, 12)).toEqual(['story']);
		expect(rulesAt(BAD, 13)).toEqual(['project']);
		expect(rulesAt(BAD, 14)).toEqual(['task']);
		expect(rulesAt(BAD, 15)).toEqual(['agent']);
		expect(rulesAt(BAD, 16)).toEqual(['pipe']);
		expect(rulesAt(BAD, 17)).toEqual(['field-spacing']);
		expect(rulesAt(BAD, 19)).toEqual(['nested-bullet']);
		expect(
			findings.filter(
				(finding) => finding.relativePath === BAD && finding.line === 18,
			),
		).toEqual([]);
	});

	it('reports file:line with the broken rule', () => {
		expect(
			findings.every(
				(finding) =>
					finding.line >= 1 &&
					finding.rule.length > 0 &&
					finding.message.length > 0,
			),
		).toBe(true);
	});
});
