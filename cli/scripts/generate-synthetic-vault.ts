/**
 * Writes a synthetic workspace of N story notes (plus hubs and two day logs)
 * in the shape of `core/fixtures/mock-vault`, for latency measurements.
 * Generic sample text only; nothing comes from a real vault.
 *
 * Usage: tsx scripts/generate-synthetic-vault.ts <target dir> [story count]
 */
import { mkdirSync, writeFileSync } from 'node:fs';
import path from 'node:path';

const PROJECTS = [
	{ key: 'WEB', name: 'Website' },
	{ key: 'ACME', name: 'Acme portal' },
	{ key: 'OPS', name: 'Operations' },
] as const;
const STATUSES = ['ready', 'in-progress', 'on-hold', 'done'] as const;
const PEOPLE = ['Dana', 'Lee', 'Priya', 'Morgan'] as const;

export function generateSyntheticVault(
	targetDir: string,
	storyCount: number,
): number {
	if (!Number.isInteger(storyCount) || storyCount <= 0)
		throw new Error('storyCount must be a positive integer');
	mkdirSync(path.join(targetDir, '.vaultday'), { recursive: true });
	writeFileSync(
		path.join(targetDir, '.vaultday', 'settings.json'),
		settingsJson(),
	);
	let fileCount = 1;
	for (const project of PROJECTS) {
		const hubDir = path.join(targetDir, 'Projects', project.name);
		mkdirSync(hubDir, { recursive: true });
		writeFileSync(path.join(hubDir, `${project.name}.md`), hubNote(project));
		fileCount += 1;
	}
	for (let index = 1; index <= storyCount; index += 1) {
		const project = PROJECTS[index % PROJECTS.length] ?? PROJECTS[0];
		const number = `${project.key}-${100 + index}`;
		const title = `${number} Sample story ${index}`;
		const storyDir = path.join(targetDir, 'Projects', project.name, title);
		mkdirSync(storyDir, { recursive: true });
		writeFileSync(path.join(storyDir, `${title}.md`), storyNote(number, index));
		fileCount += 1;
	}
	mkdirSync(path.join(targetDir, 'Log'), { recursive: true });
	for (const day of ['2026-10-21', '2026-10-22']) {
		writeFileSync(
			path.join(targetDir, 'Log', `${day}.md`),
			logNote(day, storyCount),
		);
		fileCount += 1;
	}
	return fileCount;
}

function settingsJson(): string {
	// Written as text: the manifest's keys are the schema's snake_case names.
	return [
		'{',
		'  "schema_version": 1,',
		'  "name": "Synthetic Studio",',
		'  "kind": "work",',
		'  "ticket_pattern": "(WEB|ACME|OPS)-\\\\d+",',
		'  "hours_per_day": 8,',
		'  "log_folder": "Log",',
		'  "features": {}',
		'}',
		'',
	].join('\n');
}

function hubNote(project: { key: string; name: string }): string {
	return `---\ntype: project\nkey: ${project.key}\nstatus: active\nnote_system: "1.0"\n---\n# ${project.name}\n\nSample hub for ${project.name}.\n`;
}

function storyNote(number: string, index: number): string {
	const status = STATUSES[index % STATUSES.length] ?? 'ready';
	const person = PEOPLE[index % PEOPLE.length] ?? 'Dana';
	const base = index * 10;
	return [
		'---',
		'type: story',
		`number: ${number}`,
		`status: ${status}`,
		`priority: ${index % 3 === 0 ? 'high' : 'normal'}`,
		`sort_index: ${index}`,
		'note_system: "1.0"',
		'---',
		`# Sample story ${index}`,
		'',
		`Generic description for ${number}. Nothing here comes from a real vault.`,
		'',
		'## Background',
		'- Some context line',
		'- Another context line',
		'',
		'## Next Steps',
		`- [ ] First step of story ${index}  [id:: task-${base + 1}] [created:: 2026-10-20] [estimate:: 2h]`,
		`\t- [ ] A nested step  [id:: task-${base + 2}] [created:: 2026-10-21]`,
		`- [/] Step in progress  [id:: task-${base + 3}] [created:: 2026-10-21] [estimate:: 45m]`,
		`- [x] Finished step  [id:: task-${base + 4}] [created:: 2026-10-18] [completion:: 2026-10-20]`,
		'',
		'## Pending Input',
		`- [ ] Waiting on ${person} for a decision  [id:: task-${base + 5}] [created:: 2026-10-10] [assigned_to:: ${person}]`,
		'',
		'## Notes',
		`- 2026-10-21 Sample note for story ${index}.`,
		'',
	].join('\n');
}

function logNote(day: string, storyCount: number): string {
	const lines = Array.from({ length: 12 }, (_, slot) => {
		const index = ((slot * 7) % storyCount) + 1;
		const project = PROJECTS[index % PROJECTS.length] ?? PROJECTS[0];
		const hour = String(8 + slot).padStart(2, '0');
		return `- ${hour}:${slot % 2 === 0 ? '00' : '30'} Sample entry ${slot} [kind:: ${slot % 3 === 0 ? 'session' : 'done'}] [story:: ${project.key}-${100 + index}]`;
	});
	return `---\ntype: log\ndate: ${day}\n---\n# ${day}\n\n## Log\n${lines.join('\n')}\n`;
}

const [, , targetDir, countText] = process.argv;
if (targetDir !== undefined) {
	const count = countText === undefined ? 150 : Number(countText);
	const files = generateSyntheticVault(targetDir, count);
	console.log(`wrote ${files} files to ${targetDir}`);
}
