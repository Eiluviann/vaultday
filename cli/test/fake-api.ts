import { createHmac } from 'node:crypto';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import {
	createServer,
	type IncomingMessage,
	type Server,
	type ServerResponse,
} from 'node:http';
import type { AddressInfo } from 'node:net';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { afterEach } from 'vitest';

/** One request the fake app received. */
export type ReceivedRequest = {
	readonly method: string;
	readonly url: string;
	readonly authorization: string | undefined;
	readonly body: string;
};

/** A route's canned answer. */
export type CannedAnswer = {
	readonly status: number;
	readonly body: unknown;
};

/** A stand-in for the app's localhost API and the folder its token sits in. */
export type FakeApi = {
	readonly apiDirectory: string;
	readonly environment: Readonly<Record<string, string>>;
	readonly requests: ReceivedRequest[];
};

export const FAKE_TOKEN = 'TESTTOKENTESTTOKENTESTTOKENTESTTO';

const openServers: Server[] = [];
const temporaryDirectories: string[] = [];

afterEach(async () => {
	await Promise.all(openServers.splice(0).map(closeServer));
	for (const directory of temporaryDirectories.splice(0)) {
		rmSync(directory, { recursive: true, force: true });
	}
});

/**
 * Serves `answers` (keyed `METHOD /route`) on a loopback port and writes the
 * token and port files the CLI reads. A request without the fake token gets
 * the app's 401.
 */
export async function startFakeApi(
	answers: Readonly<Record<string, CannedAnswer>>,
): Promise<FakeApi> {
	const requests: ReceivedRequest[] = [];
	const server = createServer((request, response) => {
		void collect(request).then((body) => {
			if (request.url === '/v1/hello') {
				respond(response, {
					status: 200,
					body: { proof: helloProof(request) },
				});
				return;
			}
			requests.push({
				method: request.method ?? '',
				url: request.url ?? '',
				authorization: request.headers.authorization,
				body,
			});
			answer(request, response, answers);
		});
	});
	openServers.push(server);
	const port = await listen(server);
	const apiDirectory = writeApiFiles(FAKE_TOKEN, port);
	return { apiDirectory, environment: environmentFor(apiDirectory), requests };
}

/**
 * Another program squatting on the port the stale port file names: it
 * records every request, answers anything with 200, and cannot know the token.
 */
export async function startImpostorApi(): Promise<FakeApi> {
	const requests: ReceivedRequest[] = [];
	const server = createServer((request, response) => {
		void collect(request).then((body) => {
			requests.push({
				method: request.method ?? '',
				url: request.url ?? '',
				authorization: request.headers.authorization,
				body,
			});
			respond(response, {
				status: 200,
				body: { proof: '00'.repeat(32), workspace: 'Impostor', day: {} },
			});
		});
	});
	openServers.push(server);
	const port = await listen(server);
	const apiDirectory = writeApiFiles(FAKE_TOKEN, port);
	return { apiDirectory, environment: environmentFor(apiDirectory), requests };
}

/** Token and port files naming a port where nothing listens. */
export async function closedApi(): Promise<FakeApi> {
	const server = createServer();
	const port = await listen(server);
	await closeServer(server);
	const apiDirectory = writeApiFiles(FAKE_TOKEN, port);
	return {
		apiDirectory,
		environment: environmentFor(apiDirectory),
		requests: [],
	};
}

/** The token file names a different token than the fake app expects. */
export function withWrongToken(api: FakeApi): FakeApi {
	writeFileSync(path.join(api.apiDirectory, 'token'), 'not-the-token\n');
	return api;
}

function answer(
	request: IncomingMessage,
	response: ServerResponse,
	answers: Readonly<Record<string, CannedAnswer>>,
): void {
	if (request.headers.authorization !== `Bearer ${FAKE_TOKEN}`) {
		respond(response, { status: 401, body: { error: 'Unauthorized' } });
		return;
	}
	const canned = answers[`${request.method} ${request.url}`];
	respond(
		response,
		canned ?? { status: 404, body: { error: 'No such route' } },
	);
}

/** The app's proof that it holds the token, computed apart from the CLI's own code. */
function helloProof(request: IncomingMessage): string {
	const challenge = request.headers['x-vaultday-challenge'];
	return createHmac('sha256', FAKE_TOKEN)
		.update(
			`vaultday-server-proof\n${typeof challenge === 'string' ? challenge : ''}`,
		)
		.digest('hex');
}

function respond(response: ServerResponse, canned: CannedAnswer): void {
	response.writeHead(canned.status, { 'content-type': 'application/json' });
	response.end(JSON.stringify(canned.body));
}

function collect(request: IncomingMessage): Promise<string> {
	return new Promise((resolve) => {
		let body = '';
		request.setEncoding('utf8');
		request.on('data', (chunk: string) => {
			body += chunk;
		});
		request.on('end', () => resolve(body));
	});
}

function listen(server: Server): Promise<number> {
	return new Promise((resolve) => {
		server.listen(0, '127.0.0.1', () => {
			const address: AddressInfo | string | null = server.address();
			resolve(
				typeof address === 'object' && address !== null ? address.port : 0,
			);
		});
	});
}

function closeServer(server: Server): Promise<void> {
	return new Promise((resolve) => {
		server.close(() => resolve());
		server.closeAllConnections();
	});
}

function writeApiFiles(token: string, port: number): string {
	const directory = mkdtempSync(path.join(tmpdir(), 'vaultday-api-'));
	temporaryDirectories.push(directory);
	writeFileSync(path.join(directory, 'token'), `${token}\n`, { mode: 0o600 });
	writeFileSync(path.join(directory, 'port'), `${port}\n`, { mode: 0o600 });
	return directory;
}

function environmentFor(
	apiDirectory: string,
): Readonly<Record<string, string>> {
	return Object.fromEntries([['VAULTDAY_API_DIR', apiDirectory]]);
}
