import type { LogEntry } from './day-log.ts';
import type { PlannedStory } from './notes.ts';

/** A day log as planned: `day` is the offset from the focus week's Monday. */
export type PlannedDayLog = {
	day: number;
	entries: LogEntry[];
};

// Day offsets below count from the focus week's Monday (0); the previous week is -7 to -3.

/** The stories the playground adds to the mock vault's four. */
export const NEW_STORIES: PlannedStory[] = [
	{
		number: 'WEB-163',
		title: 'Product page performance',
		projectFolder: 'Website',
		sourceId: '10163',
		status: 'in-progress',
		priority: 'high',
		due: 11,
		estimate: '1.5d',
		sortIndex: 30,
		summary:
			'Bring the product page under two seconds to interactive on a mid-range phone.',
		nextSteps: [
			{
				id: 'task-15',
				text: 'Profile the product page on a mid-range phone',
				created: -5,
				completed: 0,
				estimate: '2h',
			},
			{
				id: 'task-16',
				text: 'Lazy-load the review carousel',
				created: 0,
				scheduled: 3,
				estimate: '3h',
			},
			{
				id: 'task-17',
				text: 'Serve product images in AVIF with a WebP fallback',
				created: 0,
				completed: 4,
				estimate: '2h',
			},
			{
				id: 'task-18',
				text: 'Split the product page bundle',
				created: 1,
				due: 9,
				estimate: '4h',
			},
		],
		pendingInput: [
			{
				id: 'task-19',
				text: 'Hero image sizes from the design team',
				created: -6,
				assignedTo: 'Priya',
			},
			{
				id: 'task-20',
				text: 'Access to the CDN dashboard',
				created: 1,
				assignedTo: 'Sam',
			},
		],
		notes: [[4, 'Decided: AVIF first, WebP fallback, no JPEG.']],
	},
	{
		number: 'WEB-171',
		title: 'Accessibility fixes',
		projectFolder: 'Website',
		sourceId: '10171',
		status: 'ready',
		priority: 'medium',
		estimate: '1d',
		sortIndex: 40,
		summary:
			'The fixes from the agency audit, in the order the report ranks them.',
		nextSteps: [
			{
				id: 'task-21',
				text: 'Fix the focus order in the mobile menu',
				created: -7,
				estimate: '1h',
			},
			{
				id: 'task-22',
				text: 'Add a label to the search field',
				created: -7,
				completed: 2,
				estimate: '30m',
			},
			{
				id: 'task-23',
				text: 'Contrast pass on the footer links',
				created: -7,
				estimate: '1h',
			},
		],
		pendingInput: [
			{
				id: 'task-24',
				text: 'The audit report as a PDF from the agency',
				created: -20,
				assignedTo: 'Robin',
			},
		],
	},
	{
		number: 'ACME-15',
		title: 'Support ticket form',
		projectFolder: 'Acme portal',
		sourceId: '20015',
		status: 'in-progress',
		priority: 'high',
		due: 4,
		estimate: '2d',
		sortIndex: 40,
		summary:
			'A ticket form under Help that sends straight to the support inbox.',
		nextSteps: [
			{
				id: 'task-25',
				text: 'Build the form with file upload',
				created: -6,
				completed: 2,
				estimate: '4h',
			},
			{
				id: 'task-26',
				text: 'Validate the attachment size on the client',
				created: -6,
				completed: 4,
				estimate: '1h',
			},
			{
				id: 'task-27',
				text: 'Send the ticket to the support inbox API',
				created: -6,
				scheduled: 7,
				estimate: '3h',
			},
		],
		pendingInput: [
			{
				id: 'task-28',
				text: 'API credentials for the support inbox',
				created: -2,
				assignedTo: 'Morgan',
			},
		],
		notes: [
			[-6, 'The form lives under Help, not Account.'],
			[
				1,
				'Uploads go straight to the support inbox; the portal stores nothing.',
			],
		],
	},
	{
		number: 'ACME-18',
		title: 'Usage dashboard',
		projectFolder: 'Acme portal',
		sourceId: '20018',
		status: 'draft',
		priority: 'low',
		sortIndex: 50,
		summary:
			'A monthly usage view for portal admins; no estimate until the metrics are agreed.',
		nextSteps: [
			{ id: 'task-29', text: 'Sketch the dashboard layout', created: -2 },
		],
		pendingInput: [
			{
				id: 'task-30',
				text: 'Which metrics matter for the monthly review',
				created: -9,
				assignedTo: 'Morgan',
			},
		],
	},
];

/** The two weeks of work: the previous week (Friday on leave) and the focus week. */
export function mockStudioDayLogs(): PlannedDayLog[] {
	const web142 = { story: 'WEB-142' };
	const web157 = { story: 'WEB-157' };
	const web163 = { story: 'WEB-163' };
	const web171 = { story: 'WEB-171' };
	const acme9 = { story: 'ACME-9' };
	const acme15 = { story: 'ACME-15' };
	const website = { project: 'Website' };
	return [
		{
			day: -7,
			entries: [
				{
					kind: 'status',
					time: '09:05',
					text: 'Back on the checkout redesign after the release',
					subject: web142,
				},
				{
					kind: 'session',
					start: '09:30',
					minutes: 150,
					text: 'Outline the five mails',
					subject: acme9,
					taskId: 'task-12',
				},
				{
					kind: 'session',
					start: '13:00',
					minutes: 120,
					text: 'Checkout competitor sweep',
					subject: web142,
				},
				{
					kind: 'session',
					start: '15:30',
					minutes: 90,
					text: 'Walk through the accessibility audit',
					subject: web171,
				},
			],
		},
		{
			day: -6,
			entries: [
				{
					kind: 'session',
					start: '09:00',
					minutes: 180,
					text: 'Checkout competitor sweep',
					subject: web142,
				},
				{
					kind: 'session',
					start: '13:30',
					minutes: 120,
					text: 'Support form spec review',
					subject: acme15,
				},
				{
					kind: 'decision',
					time: '15:40',
					text: 'The ticket form lives under Help, not Account',
					subject: acme15,
				},
				{
					kind: 'session',
					start: '16:00',
					minutes: 60,
					text: 'Product page baseline metrics',
					subject: web163,
				},
			],
		},
		{
			day: -5,
			entries: [
				{
					kind: 'session',
					start: '09:00',
					minutes: 120,
					text: 'Profile the product page on a mid-range phone',
					subject: web163,
					taskId: 'task-15',
				},
				{
					kind: 'session',
					start: '11:30',
					minutes: 90,
					text: 'Checkout competitor sweep',
					subject: web142,
				},
				{
					kind: 'done',
					time: '13:05',
					text: 'Checkout research written up',
					subject: web142,
					agent: 'claude-fable-5-1',
				},
				{
					kind: 'session',
					start: '14:00',
					minutes: 150,
					text: 'Support form spec review',
					subject: acme15,
				},
			],
		},
		{
			day: -4,
			entries: [
				{
					kind: 'session',
					start: '09:00',
					minutes: 180,
					text: 'Checkout flow prototype',
					subject: web142,
				},
				{
					kind: 'session',
					start: '13:00',
					minutes: 120,
					text: 'Outline the five mails',
					subject: acme9,
					taskId: 'task-12',
				},
				{
					kind: 'session',
					start: '15:30',
					minutes: 90,
					text: 'Newsletter provider shortlist',
					subject: web157,
				},
				{
					kind: 'note',
					time: '17:10',
					text: 'Off tomorrow; Lee has the refund question',
					subject: web142,
				},
			],
		},
		{
			day: 0,
			entries: [
				{
					kind: 'status',
					time: '09:00',
					text: 'Planned the week: checkout first, then the product page',
					subject: website,
				},
				{
					kind: 'session',
					start: '09:15',
					minutes: 75,
					text: 'Profile the product page on a mid-range phone',
					subject: web163,
					taskId: 'task-15',
				},
				{
					kind: 'done',
					time: '10:31',
					text: 'Product page profiled: the review carousel blocks the main thread',
					subject: web163,
					taskId: 'task-15',
				},
				{
					kind: 'session',
					start: '10:45',
					minutes: 80,
					text: 'Outline the five mails',
					subject: acme9,
					taskId: 'task-12',
				},
				{
					kind: 'status',
					time: '12:10',
					text: 'On hold until the brand voice guide arrives',
					subject: acme9,
				},
				{
					kind: 'session',
					start: '13:00',
					minutes: 90,
					text: 'Build the form with file upload',
					subject: acme15,
					taskId: 'task-25',
				},
				{
					kind: 'session',
					start: '14:45',
					minutes: 105,
					text: 'Wire the payment form to the gateway sandbox',
					subject: web142,
					taskId: 'task-2',
				},
				{
					kind: 'session',
					start: '16:45',
					minutes: 45,
					text: 'Handle the declined-card response',
					subject: web142,
					taskId: 'task-4',
				},
			],
		},
		{
			day: 1,
			entries: [
				{
					kind: 'session',
					start: '09:00',
					minutes: 120,
					text: 'Extract the address form into its own component',
					subject: web142,
					taskId: 'task-1',
				},
				{
					kind: 'done',
					time: '11:01',
					text: 'Address form extracted into its own component',
					subject: web142,
					taskId: 'task-1',
				},
				{
					kind: 'session',
					start: '11:15',
					minutes: 60,
					text: 'Handle the declined-card response',
					subject: web142,
					taskId: 'task-4',
				},
				{
					kind: 'decision',
					time: '12:20',
					text: 'Uploads go straight to the support inbox; the portal stores nothing',
					subject: acme15,
				},
				{
					kind: 'session',
					start: '13:00',
					minutes: 120,
					text: 'Build the form with file upload',
					subject: acme15,
					taskId: 'task-25',
				},
				{
					kind: 'session',
					start: '15:15',
					minutes: 90,
					text: 'Split the product page bundle',
					subject: web163,
					taskId: 'task-18',
				},
				{
					kind: 'note',
					time: '16:50',
					text: 'Sam will grant access to the CDN dashboard',
					subject: web163,
					taskId: 'task-20',
				},
			],
		},
		{
			day: 2,
			entries: [
				{
					kind: 'session',
					start: '08:30',
					minutes: 30,
					text: 'Add a label to the search field',
					subject: web171,
					taskId: 'task-22',
				},
				{
					kind: 'done',
					time: '09:01',
					text: 'Search field labelled',
					subject: web171,
					taskId: 'task-22',
				},
				{
					kind: 'status',
					time: '09:10',
					text: 'Planned the checkout work',
					subject: web142,
				},
				{
					kind: 'decision',
					time: '10:00',
					text: 'The coupon field stays above the fold',
					subject: web142,
				},
				{
					kind: 'session',
					start: '10:10',
					minutes: 90,
					text: 'Wire the payment form to the gateway sandbox',
					subject: web142,
					taskId: 'task-2',
				},
				{
					kind: 'blocker',
					time: '12:30',
					text: 'The gateway sandbox returns 502 on refunds',
					subject: web142,
				},
				{
					kind: 'session',
					start: '13:30',
					minutes: 60,
					text: 'Build the form with file upload',
					subject: acme15,
					taskId: 'task-25',
				},
				{
					kind: 'done',
					time: '14:31',
					text: 'Ticket form built with file upload',
					subject: acme15,
					taskId: 'task-25',
				},
				{
					kind: 'session',
					start: '14:35',
					minutes: 45,
					text: 'Write the end-to-end checkout test',
					subject: web142,
					taskId: 'task-5',
				},
				{
					kind: 'session',
					start: '15:30',
					minutes: 120,
					text: 'Lazy-load the review carousel',
					subject: web163,
					taskId: 'task-16',
				},
			],
		},
		{
			day: 3,
			entries: [
				{
					kind: 'note',
					time: '08:45',
					text: 'Reviewed the address form change with Dana',
					subject: web142,
					taskId: 'task-1',
				},
				{
					kind: 'session',
					start: '09:00',
					minutes: 90,
					text: 'Write the end-to-end checkout test',
					subject: web142,
					taskId: 'task-5',
				},
				{
					kind: 'session',
					start: '10:45',
					minutes: 75,
					text: 'Lazy-load the review carousel',
					subject: web163,
					taskId: 'task-16',
				},
				{
					kind: 'done',
					time: '12:05',
					text: 'Testing steps drafted',
					subject: website,
					agent: 'claude-fable-5-1',
				},
				{
					kind: 'session',
					start: '13:00',
					minutes: 60,
					text: 'Draft the confirmation mail',
					subject: web157,
					taskId: 'task-8',
				},
				{
					kind: 'note',
					time: '14:05',
					text: 'Asked Lee again about the provider',
					subject: web157,
					taskId: 'task-9',
				},
				{
					kind: 'session',
					start: '14:15',
					minutes: 105,
					text: 'Validate the attachment size on the client',
					subject: acme15,
					taskId: 'task-26',
				},
				{
					kind: 'blocker',
					time: '16:05',
					text: 'No API credentials for the support inbox yet; sending is blocked',
					subject: acme15,
					taskId: 'task-28',
				},
				{
					kind: 'session',
					start: '16:15',
					minutes: 60,
					text: 'Wire the payment form to the gateway sandbox',
					subject: web142,
					taskId: 'task-2',
				},
			],
		},
		{
			day: 4,
			entries: [
				{
					kind: 'session',
					start: '09:00',
					minutes: 120,
					text: 'Serve product images in AVIF with a WebP fallback',
					subject: web163,
					taskId: 'task-17',
				},
				{
					kind: 'done',
					time: '11:01',
					text: 'Product images served as AVIF with a WebP fallback',
					subject: web163,
					taskId: 'task-17',
				},
				{
					kind: 'session',
					start: '11:15',
					minutes: 45,
					text: 'Fix the focus order in the mobile menu',
					subject: web171,
					taskId: 'task-21',
				},
				{
					kind: 'session',
					start: '13:00',
					minutes: 90,
					text: 'Write the end-to-end checkout test',
					subject: web142,
					taskId: 'task-5',
				},
				{
					kind: 'session',
					start: '14:45',
					minutes: 60,
					text: 'Validate the attachment size on the client',
					subject: acme15,
					taskId: 'task-26',
				},
				{
					kind: 'done',
					time: '15:50',
					text: 'Attachment size validated on the client',
					subject: acme15,
					taskId: 'task-26',
				},
				{
					kind: 'decision',
					time: '16:00',
					text: 'Product images: AVIF first, WebP fallback, no JPEG',
					subject: web163,
				},
				{
					kind: 'status',
					time: '16:30',
					text: 'Week wrap: checkout test green locally, refunds still blocked on the sandbox',
					subject: website,
				},
			],
		},
	];
}
