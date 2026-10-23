import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { InMemoryTransport } from '@modelcontextprotocol/sdk/inMemory.js';
import { describe, expect, it } from 'vitest';
import { createVaultdayServer } from '../src/mcp-server.ts';
import {
	closedApi,
	FAKE_TOKEN,
	startFakeApi,
	startImpostorApi,
	withWrongToken,
} from './fake-api.ts';
import { emptyDirectory, runVaultday } from './support.ts';

const DAY = {
	date: '2026-10-23',
	spentSeconds: 11_400,
	loggedSeconds: 9000,
	workloadSeconds: 28_800,
};

const RUNNING = {
	workspace: 'Mock Studio',
	timer: {
		sessionId: 'ses-41',
		taskId: 'task-5',
		story: 'WEB-142',
		text: 'Write the end-to-end checkout test',
		isPaused: false,
		startedAt: '2026-10-23T09:30:00Z',
		elapsedSeconds: 2832,
	},
	day: DAY,
};

const IDLE = { workspace: 'Mock Studio', day: DAY };

const LOG_LINE =
	'- 09:30 session · Write the end-to-end checkout test · 47m [story:: WEB-142] [kind:: session] [id:: ses-41] [task:: task-5]';

describe('vaultday timer', () => {
	it('starts a task by id or text with the bearer token and prints what runs', async () => {
		const api = await startFakeApi({
			'POST /v1/timer/start': { status: 201, body: RUNNING },
		});
		const run = await runVaultday(
			['timer', 'start', 'end-to-end', 'test', '--ago', '10'],
			emptyDirectory(),
			api.environment,
		);
		expect(run.error).toBe('');
		expect(run.code).toBe(0);
		expect(run.out).toBe(
			'Started · task-5 · Write the end-to-end checkout test\n',
		);
		expect(api.requests).toEqual([
			{
				method: 'POST',
				url: '/v1/timer/start',
				authorization: `Bearer ${FAKE_TOKEN}`,
				body: JSON.stringify({ target: 'end-to-end test', minutesAgo: 10 }),
			},
		]);
	});

	it('stops the session and prints the line the app logged', async () => {
		const api = await startFakeApi({
			'POST /v1/timer/stop': {
				status: 200,
				body: { ...IDLE, logLine: LOG_LINE },
			},
		});
		const run = await runVaultday(
			['timer', 'stop'],
			emptyDirectory(),
			api.environment,
		);
		expect(run.code).toBe(0);
		expect(run.out).toBe(`Stopped · logged\n${LOG_LINE}\n`);
		expect(api.requests.map((request) => request.url)).toEqual([
			'/v1/timer/stop',
		]);
	});

	it('continues the latest session and prints JSON on request', async () => {
		const api = await startFakeApi({
			'POST /v1/timer/continue': { status: 201, body: RUNNING },
		});
		const run = await runVaultday(
			['timer', 'continue', '--json'],
			emptyDirectory(),
			api.environment,
		);
		expect(run.code).toBe(0);
		expect(JSON.parse(run.out)).toEqual(RUNNING);
	});

	it('says the app is not running when the socket is closed', async () => {
		const api = await closedApi();
		const run = await runVaultday(
			['timer', 'stop'],
			emptyDirectory(),
			api.environment,
		);
		expect(run.code).toBe(1);
		expect(run.error).toBe('vaultday: Vaultday is not running\n');
	});

	it('says the app is not running when it never wrote its token', async () => {
		const run = await runVaultday(
			['status'],
			emptyDirectory(),
			Object.fromEntries([['VAULTDAY_API_DIR', emptyDirectory()]]),
		);
		expect(run.code).toBe(1);
		expect(run.error).toBe('vaultday: Vaultday is not running\n');
	});

	it('says the app refused the token on 401', async () => {
		const api = withWrongToken(
			await startFakeApi({ 'GET /v1/status': { status: 200, body: IDLE } }),
		);
		const run = await runVaultday(
			['status'],
			emptyDirectory(),
			api.environment,
		);
		expect(run.code).toBe(1);
		expect(run.error).toBe('vaultday: Vaultday refused the token\n');
	});

	it('never sends the token to a program on the port that cannot prove it is Vaultday', async () => {
		const impostor = await startImpostorApi();
		const run = await runVaultday(
			['timer', 'stop'],
			emptyDirectory(),
			impostor.environment,
		);
		expect(run.code).toBe(1);
		expect(run.error).toBe('vaultday: Vaultday refused the token\n');
		expect(
			impostor.requests.map((request) => request.authorization),
		).not.toContain(`Bearer ${FAKE_TOKEN}`);
		expect(impostor.requests.map((request) => request.url)).toEqual([
			'/v1/hello',
		]);
	});

	it("passes the app's own refusal through", async () => {
		const api = await startFakeApi({
			'POST /v1/timer/start': {
				status: 404,
				body: { error: 'No open task matches "lunch"' },
			},
		});
		const run = await runVaultday(
			['timer', 'start', 'lunch'],
			emptyDirectory(),
			api.environment,
		);
		expect(run.code).toBe(1);
		expect(run.error).toBe('vaultday: No open task matches "lunch"\n');
	});

	it('refuses a start without a target and a bad --ago as usage errors', async () => {
		const missing = await runVaultday(['timer', 'start'], emptyDirectory());
		expect(missing.code).toBe(2);
		const ago = await runVaultday(
			['timer', 'start', 'task-5', '--ago', 'soon'],
			emptyDirectory(),
		);
		expect(ago.code).toBe(2);
		expect(ago.error).toContain('--ago takes whole minutes');
	});
});

describe('vaultday status', () => {
	it('prints the running timer and the day in text', async () => {
		const api = await startFakeApi({
			'GET /v1/status': { status: 200, body: RUNNING },
		});
		const run = await runVaultday(
			['status'],
			emptyDirectory(),
			api.environment,
		);
		expect(run.out).toBe(
			'Running · 0:47:12 · task-5 · Write the end-to-end checkout test\nLogged 2h 30m of 8h today · tracked 3h 10m\n',
		);
	});

	it('prints the sketchybar format: one key=value per line, nothing quoted', async () => {
		const api = await startFakeApi({
			'GET /v1/status': { status: 200, body: RUNNING },
		});
		const run = await runVaultday(
			['status', '--format', 'sketchybar'],
			emptyDirectory(),
			api.environment,
		);
		expect(run.code).toBe(0);
		expect(run.out).toBe(
			[
				'state=running',
				'elapsed=0:47:12',
				'task=task-5',
				'story=WEB-142',
				'text=Write the end-to-end checkout test',
				'label=0:47:12 task-5 · Write the end-to-end checkout test',
				'logged=2h30',
				'workload=8h',
				'ring=2h30 / 8h',
				'workspace=Mock Studio',
				'',
			].join('\n'),
		);
	});

	it('prints an idle sketchybar status with empty values', async () => {
		const api = await startFakeApi({
			'GET /v1/status': { status: 200, body: IDLE },
		});
		const run = await runVaultday(
			['status', '--format', 'sketchybar'],
			emptyDirectory(),
			api.environment,
		);
		expect(run.out.split('\n').slice(0, 3)).toEqual([
			'state=idle',
			'elapsed=',
			'task=',
		]);
	});

	it('keeps a paused timer and a multi-line text on one line each', async () => {
		const paused = {
			...RUNNING,
			timer: { ...RUNNING.timer, isPaused: true, text: 'Two\nlines' },
		};
		const api = await startFakeApi({
			'GET /v1/status': { status: 200, body: paused },
		});
		const run = await runVaultday(
			['status', '--format', 'sketchybar'],
			emptyDirectory(),
			api.environment,
		);
		expect(run.out).toContain('state=paused\n');
		expect(run.out).toContain('text=Two lines\n');
	});

	it('refuses an unknown format', async () => {
		const run = await runVaultday(
			['status', '--format', 'xml'],
			emptyDirectory(),
		);
		expect(run.code).toBe(2);
	});
});

describe('the MCP timer tools', () => {
	async function connect(environment: Readonly<Record<string, string>>) {
		const [clientSide, serverSide] = InMemoryTransport.createLinkedPair();
		await createVaultdayServer({
			workingDirectory: emptyDirectory(),
			environment,
		}).connect(serverSide);
		const client = new Client({ name: 'test', version: '0.0.0' });
		await client.connect(clientSide);
		return client;
	}

	function textOf(result: Awaited<ReturnType<Client['callTool']>>): string {
		const [first] = Array.isArray(result.content) ? result.content : [];
		return first?.type === 'text' ? first.text : '';
	}

	it('start, stop, continue and status go to the API', async () => {
		const api = await startFakeApi({
			'POST /v1/timer/start': { status: 201, body: RUNNING },
			'POST /v1/timer/stop': {
				status: 200,
				body: { ...IDLE, logLine: LOG_LINE },
			},
			'POST /v1/timer/continue': { status: 201, body: RUNNING },
			'GET /v1/status': { status: 200, body: RUNNING },
		});
		const client = await connect(api.environment);
		const started = await client.callTool({
			name: 'timer_start',
			arguments: { target: 'task-5' },
		});
		expect(textOf(started)).toContain('task-5');
		const stopped = await client.callTool({
			name: 'timer_stop',
			arguments: {},
		});
		expect(textOf(stopped)).toContain(LOG_LINE);
		await client.callTool({ name: 'timer_continue', arguments: {} });
		const status = await client.callTool({ name: 'status', arguments: {} });
		expect(JSON.parse(textOf(status))).toEqual(RUNNING);
		expect(
			api.requests.map((request) => `${request.method} ${request.url}`),
		).toEqual([
			'POST /v1/timer/start',
			'POST /v1/timer/stop',
			'POST /v1/timer/continue',
			'GET /v1/status',
		]);
	});

	it('answers an error when the app is not running', async () => {
		const api = await closedApi();
		const client = await connect(api.environment);
		const result = await client.callTool({ name: 'status', arguments: {} });
		expect(result.isError).toBe(true);
		expect(textOf(result)).toBe('Vaultday is not running');
	});
});
