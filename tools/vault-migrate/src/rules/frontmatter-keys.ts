/** Old key → new key; the value is kept as written. */
export const FRONTMATTER_KEY_RENAMES: ReadonlyArray<readonly [string, string]> =
	[
		['Number', 'number'],
		['Ticket', 'ticket'],
		['Status', 'status'],
		['note-system-ver', 'note_system'],
	];

export const NOTE_SYSTEM_KEY = 'note_system';
/** Migrated notes stay on the old system version: their facts weigh less than new notes until the user bumps them by hand. */
export const LEGACY_NOTE_SYSTEM_VERSION = '0.1';
export const STORY_EMPTY_KEYS = ['priority', 'due', 'estimate'] as const;
export const DEFAULT_SOURCE_KIND = 'rm_story';

export type TicketIdentity = { sourceHost: string; sourceKind: string };

/**
 * Pulls the ServiceNow host and table out of a `ticket` markdown link such as
 * `[Open in ServiceNow](https://host/nav_to.do?uri=rm_story.do%3F…)`. The URL
 * carries no sys_id, so `source_id` stays empty.
 */
export function parseTicketIdentity(ticketValue: string): TicketIdentity {
	const urlMatch =
		/\((https?:\/\/[^)\s]+)\)/.exec(ticketValue) ??
		/(https?:\/\/\S+)/.exec(ticketValue);
	const rawUrl = urlMatch?.[1];
	if (rawUrl === undefined) return { sourceHost: '', sourceKind: '' };
	let host = '';
	try {
		host = new URL(rawUrl).host;
	} catch {
		host = '';
	}
	const decoded = decodeURIComponent(rawUrl);
	const kindMatch = /uri=([a-z0-9_]+)\.do/.exec(decoded);
	return {
		sourceHost: host,
		sourceKind: kindMatch?.[1] ?? (host.length > 0 ? DEFAULT_SOURCE_KIND : ''),
	};
}
