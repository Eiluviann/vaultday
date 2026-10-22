import { type Frontmatter, splitFrontmatterBlock } from './frontmatter.ts';
import {
	currentTexts,
	type NoteDocument,
	replaceLineRange,
} from './note-document.ts';
import type { TypeGuess } from './profile.ts';
import {
	FRONTMATTER_KEY_RENAMES,
	LEGACY_NOTE_SYSTEM_VERSION,
	NOTE_SYSTEM_KEY,
	parseTicketIdentity,
	STORY_EMPTY_KEYS,
	TEMPLATE_NOTE_SYSTEM_VERSION,
} from './rules/frontmatter-keys.ts';
import { normaliseStatus } from './rules/status-vocabulary.ts';

export type NoteClass =
	| 'story'
	| 'hub'
	| 'personal-project'
	| 'template'
	| 'other';

export type FrontmatterRuleContext = {
	vaultKind: 'work' | 'personal';
	noteClass: NoteClass;
	/** Birth date of the live file; undefined when unknown or for templates. */
	birthDate: string | undefined;
	hub?: { key: string; togglProject: string | undefined };
	guess?: TypeGuess;
	/** Set when the story is renamed: the old title joins the aliases. */
	renamedFrom?: string;
};

export type FrontmatterOutcome = { reports: string[] };

/**
 * Applies the frontmatter rule table to one note. Untouched keys keep their
 * exact text; the block is re-rendered only when a rule changed something.
 */
export function transformFrontmatter(
	document: NoteDocument,
	context: FrontmatterRuleContext,
): FrontmatterOutcome {
	const texts = currentTexts(document);
	const { blockEnd, frontmatter } = splitFrontmatterBlock(texts);
	const reports: string[] = [];
	const relativePath = document.relativePath;

	for (const [oldKey, newKey] of FRONTMATTER_KEY_RENAMES) {
		if (!frontmatter.has(oldKey)) continue;
		if (frontmatter.has(newKey))
			reports.push(
				`${relativePath}: both ${oldKey} and ${newKey} present; ${oldKey} left as is`,
			);
		else frontmatter.rename(oldKey, newKey);
	}
	// Content notes keep their old version (or get 0.1 when they had none): their facts weigh less until the user bumps them by hand. Templates seed new notes and carry 1.0.
	if (context.noteClass === 'template')
		frontmatter.setScalar(NOTE_SYSTEM_KEY, TEMPLATE_NOTE_SYSTEM_VERSION);
	else if (!frontmatter.has(NOTE_SYSTEM_KEY))
		frontmatter.setScalar(NOTE_SYSTEM_KEY, LEGACY_NOTE_SYSTEM_VERSION);
	normaliseStatusValue(frontmatter);

	const isStoryShaped = frontmatter.has('number');
	if (
		context.noteClass === 'story' ||
		(context.noteClass === 'template' && isStoryShaped)
	) {
		applyStoryRules(frontmatter, context, relativePath, reports);
	} else if (context.noteClass === 'hub') {
		applyHubRules(frontmatter, context);
	} else if (context.noteClass === 'personal-project') {
		if (!frontmatter.has('type'))
			frontmatter.setScalar('type', 'project', 'start');
	} else if (context.noteClass === 'other') {
		applyTypeGuess(frontmatter, context.guess, relativePath, reports);
	}

	if (context.noteClass !== 'template' && !frontmatter.has('created')) {
		if (context.birthDate === undefined)
			reports.push(`${relativePath}: no birthtime known; created not set`);
		else
			frontmatter.setScalar('created', context.birthDate, {
				before: NOTE_SYSTEM_KEY,
			});
	}

	const rendered = frontmatter.render();
	const newBlock = ['---', ...rendered, '---'];
	if (blockEnd === 0) {
		replaceLineRange(document, 0, 0, newBlock, 'frontmatter');
	} else {
		replaceLineRange(document, 0, blockEnd, newBlock, 'frontmatter');
	}
	return { reports };
}

function normaliseStatusValue(frontmatter: Frontmatter): void {
	const status = frontmatter.scalar('status');
	if (status === undefined || status.length === 0 || status.includes('<%'))
		return;
	const normalised = normaliseStatus(status);
	if (normalised !== status) frontmatter.setScalar('status', normalised);
}

function applyStoryRules(
	frontmatter: Frontmatter,
	context: FrontmatterRuleContext,
	relativePath: string,
	reports: string[],
): void {
	if (!frontmatter.has('type')) frontmatter.setScalar('type', 'story', 'start');
	if (frontmatter.has('ticket')) {
		const ticket = frontmatter.scalar('ticket') ?? '';
		const identity = parseTicketIdentity(ticket);
		if (!frontmatter.has('source_host'))
			frontmatter.setScalar('source_host', identity.sourceHost, {
				after: 'ticket',
			});
		if (!frontmatter.has('source_kind'))
			frontmatter.setScalar('source_kind', identity.sourceKind, {
				after: 'source_host',
			});
		if (!frontmatter.has('source_id')) {
			frontmatter.setScalar('source_id', '', { after: 'source_kind' });
			if (ticket.length > 0)
				reports.push(
					`${relativePath}: source_id left empty (ticket URL carries no sys_id)`,
				);
		}
	}
	let previous = frontmatter.has('status') ? 'status' : 'type';
	for (const key of STORY_EMPTY_KEYS) {
		if (!frontmatter.has(key))
			frontmatter.setScalar(key, '', { after: previous });
		previous = key;
	}
	if (context.noteClass === 'template') return;
	const number = frontmatter.scalar('number') ?? '';
	if (number.length === 0) return;
	const aliases = frontmatter.list('aliases');
	const wanted = [
		number,
		...(context.renamedFrom === undefined ? [] : [context.renamedFrom]),
	];
	const merged = [
		...aliases,
		...wanted.filter((alias) => !aliases.includes(alias)),
	];
	if (merged.length !== aliases.length)
		frontmatter.setList(
			'aliases',
			merged,
			frontmatter.has('aliases') ? 'end' : { after: 'sort_index' },
		);
}

function applyHubRules(
	frontmatter: Frontmatter,
	context: FrontmatterRuleContext,
): void {
	if (!frontmatter.has('type'))
		frontmatter.setScalar('type', 'project', 'start');
	if (context.hub === undefined) return;
	if (!frontmatter.has('key'))
		frontmatter.setScalar('key', context.hub.key, { after: 'type' });
	if (!frontmatter.has('status'))
		frontmatter.setScalar('status', 'active', { after: 'key' });
	if (!frontmatter.has('toggl_project'))
		frontmatter.setScalar('toggl_project', context.hub.togglProject ?? '', {
			after: 'status',
		});
}

function applyTypeGuess(
	frontmatter: Frontmatter,
	guess: TypeGuess | undefined,
	relativePath: string,
	reports: string[],
): void {
	if (guess === undefined) {
		if (!frontmatter.has('type'))
			reports.push(`${relativePath}: no type rule; type left unset`);
		return;
	}
	if (frontmatter.scalar('type') !== guess.type)
		frontmatter.setScalar(
			'type',
			guess.type,
			frontmatter.has('type') ? 'end' : 'start',
		);
	if (guess.kind !== undefined && !frontmatter.has('kind'))
		frontmatter.setScalar('kind', guess.kind, { after: 'type' });
	if (guess.status !== undefined && !frontmatter.has('status'))
		frontmatter.setScalar('status', guess.status, {
			after: guess.kind === undefined ? 'type' : 'kind',
		});
	if (guess.story !== undefined && !frontmatter.has('story'))
		frontmatter.setScalar('story', guess.story, { after: 'type' });
	if (guess.project !== undefined && !frontmatter.has('project'))
		frontmatter.setRaw('project', ` "[[${guess.project}]]"`, { after: 'type' });
	if (guess.aliases !== undefined) {
		const existing = frontmatter.list('aliases');
		const merged = [
			...existing,
			...guess.aliases.filter((alias) => !existing.includes(alias)),
		];
		if (merged.length !== existing.length)
			frontmatter.setList('aliases', merged);
	}
}
