import { describe, expect, it } from 'vitest';
import { type DayLog, formatDuration, renderDayLogs } from '../src/day-log.ts';

const OPTIONS = {
	logFolder: 'Log',
	today: '2026-10-21',
	now: '12:00',
	firstSessionNumber: 7,
};

describe('renderDayLogs', () => {
	it('writes the convention: header, session lines at their stop time, numbered ids', () => {
		const days: DayLog[] = [
			{
				date: '2026-10-20',
				entries: [
					{
						kind: 'session',
						start: '10:10',
						minutes: 90,
						text: 'Wire the form',
						subject: { story: 'WEB-142' },
						taskId: 'task-2',
					},
					{
						kind: 'decision',
						time: '09:00',
						text: 'Coupon stays up',
						subject: { project: 'Website' },
					},
				],
			},
		];
		expect(renderDayLogs(days, OPTIONS)).toEqual([
			{
				path: 'Log/2026-10-20.md',
				content: [
					'---',
					'type: log',
					'date: 2026-10-20',
					'---',
					'# 2026-10-20',
					'',
					'## Log',
					'- 09:00 decision · Coupon stays up [project:: Website] [kind:: decision]',
					'- 11:40 session · Wire the form · 1h 30m [story:: WEB-142] [kind:: session] [id:: ses-7] [task:: task-2]',
					'',
				].join('\n'),
			},
		]);
	});

	it('writes nothing after now, and no file for a day still to come or left empty', () => {
		const days: DayLog[] = [
			{
				date: '2026-10-21',
				entries: [
					{
						kind: 'note',
						time: '13:00',
						text: 'Afternoon',
						subject: { story: 'WEB-142' },
					},
				],
			},
			{
				date: '2026-10-22',
				entries: [{ kind: 'note', time: '08:00', text: 'Tomorrow' }],
			},
		];
		expect(renderDayLogs(days, OPTIONS)).toEqual([]);
	});

	it('refuses two sessions that overlap', () => {
		const days: DayLog[] = [
			{
				date: '2026-10-20',
				entries: [
					{
						kind: 'session',
						start: '09:00',
						minutes: 60,
						text: 'A',
						subject: { story: 'WEB-1' },
					},
					{
						kind: 'session',
						start: '09:30',
						minutes: 30,
						text: 'B',
						subject: { story: 'WEB-1' },
					},
				],
			},
		];
		expect(() => renderDayLogs(days, OPTIONS)).toThrow('runs into');
	});
});

describe('formatDuration', () => {
	it('writes the forms the session parser reads', () => {
		expect([45, 60, 90, 125].map(formatDuration)).toEqual([
			'45m',
			'1h',
			'1h 30m',
			'2h 5m',
		]);
	});
});
