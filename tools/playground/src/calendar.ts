/** A calendar date as the vault writes it, `YYYY-MM-DD`. */
export type IsoDate = string;

/** A time of day as the day log writes it, `HH:MM`. */
export type ClockTime = string;

/**
 * The dates the playground is built around. Every generated date is an
 * offset from `focusMonday`, so a reset on any day gives a week of work that
 * ends today (or last Friday at a weekend) and waiting-on ages that read the same.
 */
export type PlaygroundCalendar = {
	today: IsoDate;
	/** The time the generator ran; today's log holds only the lines before it. */
	now: ClockTime;
	/** The Monday of the week holding today. */
	focusMonday: IsoDate;
	/** Days between the fixtures' week (Monday 2026-10-19) and the focus week. */
	fixtureShiftDays: number;
};

/** The Monday of the week the fixtures were written for. */
export const FIXTURE_MONDAY: IsoDate = '2026-10-19';

const ISO_DATE_PATTERN = /^\d{4}-\d{2}-\d{2}$/;
const CLOCK_TIME_PATTERN = /^([01]\d|2[0-3]):[0-5]\d$/;
const MILLISECONDS_PER_DAY = 86_400_000;

/**
 * Plans the playground's dates around `today`.
 *
 * @throws When `today` or `now` is not a valid date or time: callers pass parsed values.
 */
export function planCalendar(
	today: IsoDate,
	now: ClockTime,
): PlaygroundCalendar {
	if (!isValidIsoDate(today))
		throw new Error(`today is not a valid date: ${today}`);
	if (!CLOCK_TIME_PATTERN.test(now))
		throw new Error(`now is not a valid time: ${now}`);
	const focusMonday = addDays(today, -daysSinceMonday(today));
	return {
		today,
		now,
		focusMonday,
		fixtureShiftDays: daysBetween(FIXTURE_MONDAY, focusMonday),
	};
}

/** The date `days` after `date` (before it when negative). */
export function addDays(date: IsoDate, days: number): IsoDate {
	return new Date(toEpochMilliseconds(date) + days * MILLISECONDS_PER_DAY)
		.toISOString()
		.slice(0, 10);
}

/** Whole days from `from` to `to`; negative when `to` comes first. */
export function daysBetween(from: IsoDate, to: IsoDate): number {
	return Math.round(
		(toEpochMilliseconds(to) - toEpochMilliseconds(from)) /
			MILLISECONDS_PER_DAY,
	);
}

/** 0 for Monday up to 6 for Sunday. */
export function daysSinceMonday(date: IsoDate): number {
	return (new Date(toEpochMilliseconds(date)).getUTCDay() + 6) % 7;
}

/** Whether `text` is a real calendar date in ISO form (no 2026-02-30). */
export function isValidIsoDate(text: string): boolean {
	if (!ISO_DATE_PATTERN.test(text)) return false;
	const epoch = Date.parse(`${text}T00:00:00Z`);
	return (
		!Number.isNaN(epoch) && new Date(epoch).toISOString().slice(0, 10) === text
	);
}

/** Whether `text` is a time of day in `HH:MM`. */
export function isValidClockTime(text: string): boolean {
	return CLOCK_TIME_PATTERN.test(text);
}

/** `HH:MM` plus `minutes`, within the same day. */
export function addMinutes(time: ClockTime, minutes: number): ClockTime {
	const total = minutesOfDay(time) + minutes;
	if (total < 0 || total >= 24 * 60)
		throw new Error(`${time} plus ${minutes} minutes leaves the day`);
	return `${pad(Math.floor(total / 60))}:${pad(total % 60)}`;
}

/** Minutes since midnight. */
export function minutesOfDay(time: ClockTime): number {
	const [hours, minutes] = time.split(':').map(Number);
	if (hours === undefined || minutes === undefined)
		throw new Error(`not a time: ${time}`);
	return hours * 60 + minutes;
}

/** The local date and time of `moment`, as the generator's defaults. */
export function localDateAndTime(moment: Date): {
	date: IsoDate;
	time: ClockTime;
} {
	return {
		date: `${moment.getFullYear()}-${pad(moment.getMonth() + 1)}-${pad(moment.getDate())}`,
		time: `${pad(moment.getHours())}:${pad(moment.getMinutes())}`,
	};
}

function toEpochMilliseconds(date: IsoDate): number {
	const epoch = Date.parse(`${date}T00:00:00Z`);
	if (Number.isNaN(epoch)) throw new Error(`not a date: ${date}`);
	return epoch;
}

function pad(value: number): string {
	return String(value).padStart(2, '0');
}
