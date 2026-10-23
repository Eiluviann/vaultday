import { mkdirSync, utimesSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { describe, expect, it } from 'vitest';
import {
	APP_GROUP_IDENTIFIER,
	type ApiConnection,
	apiDirectoryCandidatesFor,
	NOT_RUNNING_MESSAGE,
	REFUSED_TOKEN_MESSAGE,
	readApiConnection,
} from '../src/api-client.ts';
import type { Failure } from '../src/failure.ts';
import { err, ok, type Result } from '../src/result.ts';
import { emptyDirectory } from './support.ts';

const TOKEN = 'a'.repeat(64);
const STALE_TOKEN = 'b'.repeat(64);
const SHIPPED_API = `Library/Group Containers/${APP_GROUP_IDENTIFIER}/API`;
const LOCAL_API = 'Library/Application Support/Vaultday/API';

describe('finding the running app', () => {
	it('tries the App Group, a Personal Team group, then Application Support', () => {
		const groupContainers = [
			'ABCDE12345.com.eiluviann.vaultday',
			'ABCDE12345.com.other.app',
			'group.com.other.app',
			'abcde12345.com.eiluviann.vaultday',
		];
		expect(
			apiDirectoryCandidatesFor({}, '/Users/someone', groupContainers),
		).toEqual([
			`/Users/someone/Library/Group Containers/${APP_GROUP_IDENTIFIER}/API`,
			'/Users/someone/Library/Group Containers/ABCDE12345.com.eiluviann.vaultday/API',
			'/Users/someone/Library/Application Support/Vaultday/API',
		]);
	});

	it('reads a Personal Team build from its team-prefixed App Group', async () => {
		const home = emptyDirectory();
		writeApiFiles(
			path.join(
				home,
				'Library/Group Containers/ABCDE12345.com.eiluviann.vaultday/API',
			),
			TOKEN,
			'47392',
		);

		const connection = await readApiConnection({}, home, acceptEveryServer);

		expect(connection.ok && connection.value.baseUrl).toBe(
			'http://127.0.0.1:47392',
		);
	});

	it('uses VAULTDAY_API_DIR alone when set', () => {
		expect(
			apiDirectoryCandidatesFor(
				Object.fromEntries([['VAULTDAY_API_DIR', ' /tmp/api ']]),
				'/Users/someone',
			),
		).toEqual(['/tmp/api']);
	});

	it('reads a build without the App Group from Application Support', async () => {
		const home = emptyDirectory();
		writeApiFiles(path.join(home, LOCAL_API), TOKEN, '47390');

		const connection = await readApiConnection({}, home, acceptEveryServer);

		expect(connection).toEqual({
			ok: true,
			value: { baseUrl: 'http://127.0.0.1:47390', token: TOKEN },
		});
	});

	it('skips a folder without a port file, left by a build that is not running', async () => {
		const home = emptyDirectory();
		writeApiFiles(path.join(home, SHIPPED_API), STALE_TOKEN);
		writeApiFiles(path.join(home, LOCAL_API), TOKEN, '47391');

		const connection = await readApiConnection({}, home, acceptEveryServer);

		expect(connection.ok && connection.value.baseUrl).toBe(
			'http://127.0.0.1:47391',
		);
	});

	it('skips a port a crashed build left behind when its server does not answer', async () => {
		const home = emptyDirectory();
		writeApiFiles(path.join(home, SHIPPED_API), STALE_TOKEN, '47390');
		writeApiFiles(path.join(home, LOCAL_API), TOKEN, '47393');

		const connection = await readApiConnection(
			{},
			home,
			answerOnlyWithToken(TOKEN, 'app-not-running'),
		);

		expect(connection).toEqual({
			ok: true,
			value: { baseUrl: 'http://127.0.0.1:47393', token: TOKEN },
		});
	});

	it('keeps the shipped App Group over a newer port file planted in Application Support', async () => {
		const home = emptyDirectory();
		const shipped = path.join(home, SHIPPED_API);
		writeApiFiles(shipped, TOKEN, '47390');
		const yesterday = new Date(Date.now() - 86_400_000);
		utimesSync(path.join(shipped, 'port'), yesterday, yesterday);
		writeApiFiles(path.join(home, LOCAL_API), STALE_TOKEN, '47399');

		const connection = await readApiConnection({}, home, acceptEveryServer);

		expect(connection).toEqual({
			ok: true,
			value: { baseUrl: 'http://127.0.0.1:47390', token: TOKEN },
		});
	});

	it('reports a refused token when the only listening server fails the proof', async () => {
		const home = emptyDirectory();
		writeApiFiles(path.join(home, SHIPPED_API), STALE_TOKEN, '47390');
		writeApiFiles(path.join(home, LOCAL_API), TOKEN, '47393');

		const connection = await readApiConnection(
			{},
			home,
			answerOnlyWithToken('c'.repeat(64), 'refused-token'),
		);

		expect(connection).toEqual({
			ok: false,
			error: expect.objectContaining({ message: REFUSED_TOKEN_MESSAGE }),
		});
	});

	it('reads as not running when no folder holds both files', async () => {
		const connection = await readApiConnection(
			{},
			emptyDirectory(),
			acceptEveryServer,
		);

		expect(connection).toEqual({
			ok: false,
			error: expect.objectContaining({ message: NOT_RUNNING_MESSAGE }),
		});
	});
});

async function acceptEveryServer(): Promise<Result<undefined, Failure>> {
	return ok(undefined);
}

/** A prover where only the server holding `token` answers; others fail with `otherwise`. */
function answerOnlyWithToken(
	token: string,
	otherwise: 'app-not-running' | 'refused-token',
): (connection: ApiConnection) => Promise<Result<undefined, Failure>> {
	return async (connection) => {
		if (connection.token === token) return ok(undefined);
		return otherwise === 'app-not-running'
			? err({ kind: otherwise, message: NOT_RUNNING_MESSAGE })
			: err({ kind: otherwise, message: REFUSED_TOKEN_MESSAGE });
	};
}

function writeApiFiles(directory: string, token: string, port?: string): void {
	mkdirSync(directory, { recursive: true });
	writeFileSync(path.join(directory, 'token'), `${token}\n`);
	if (port !== undefined)
		writeFileSync(path.join(directory, 'port'), `${port}\n`);
}
