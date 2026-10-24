import { describe, expect, it } from 'vitest';
import {
	addDays,
	addMinutes,
	isValidIsoDate,
	planCalendar,
} from '../src/calendar.ts';

describe('planCalendar', () => {
	it('builds a weekend run around the week that just ended', () => {
		expect(planCalendar('2026-10-24', '11:00')).toEqual({
			today: '2026-10-24',
			now: '11:00',
			focusMonday: '2026-10-19',
			fixtureShiftDays: 0,
		});
	});

	it('builds a weekday run around the week holding today', () => {
		expect(planCalendar('2026-11-04', '09:30')).toMatchObject({
			focusMonday: '2026-11-02',
			fixtureShiftDays: 14,
		});
		expect(planCalendar('2026-11-02', '09:30')).toMatchObject({
			focusMonday: '2026-11-02',
		});
	});

	it('refuses an impossible date or time', () => {
		expect(() => planCalendar('2026-02-30', '09:00')).toThrow(
			'not a valid date',
		);
		expect(() => planCalendar('2026-10-24', '24:00')).toThrow(
			'not a valid time',
		);
	});
});

describe('date and time arithmetic', () => {
	it('crosses months and years', () => {
		expect(addDays('2026-10-30', 3)).toBe('2026-11-02');
		expect(addDays('2027-01-01', -1)).toBe('2026-12-31');
	});

	it('tells real dates from look-alikes', () => {
		expect(isValidIsoDate('2028-02-29')).toBe(true);
		expect(isValidIsoDate('2026-13-01')).toBe(false);
		expect(isValidIsoDate('2026-1-01')).toBe(false);
	});

	it('keeps a time inside the day', () => {
		expect(addMinutes('09:15', 105)).toBe('11:00');
		expect(() => addMinutes('23:30', 45)).toThrow('leaves the day');
	});
});
