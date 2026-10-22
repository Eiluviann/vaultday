/** Lowercase, hyphenated status tokens: `On Hold` → `on-hold`, `In progress` → `in-progress`. */
export function normaliseStatus(value: string): string {
	return value.trim().toLowerCase().replace(/\s+/g, '-');
}

export const STORY_STATUSES = [
	'draft',
	'ready',
	'in-progress',
	'on-hold',
	'done',
	'cancelled',
] as const;
export const Work_PROJECT_STATUSES = [
	'active',
	'on-hold',
	'done',
	'cancelled',
] as const;
export const PERSONAL_PROJECT_STATUSES = [
	'idea',
	'active',
	'paused',
	'shipped',
	'abandoned',
] as const;
