import { createHmac, randomBytes, timingSafeEqual } from 'node:crypto';
import { readdirSync } from 'node:fs';
import { readFile } from 'node:fs/promises';
import { homedir } from 'node:os';
import path from 'node:path';
import type { Failure, FailureKind } from './failure.ts';
import type { Environment } from './provenance.ts';
import { err, ok, type Result } from './result.ts';

/** How the CLI reaches the running app: its loopback port and bearer token. */
export type ApiConnection = {
	readonly baseUrl: string;
	readonly token: string;
};

export type ApiMethod = 'GET' | 'POST';

/**
 * Asks the server behind `connection` to prove it holds the token, before
 * the token is ever sent to it.
 */
export type ServerProver = (
	connection: ApiConnection,
) => Promise<Result<undefined, Failure>>;

export const APP_GROUP_IDENTIFIER = 'group.com.eiluviann.vaultday';
export const NOT_RUNNING_MESSAGE = 'Vaultday is not running';
export const REFUSED_TOKEN_MESSAGE = 'Vaultday refused the token';

const API_DIRECTORY_ENVIRONMENT_KEY = 'VAULTDAY_API_DIR';
/** A Personal Team build's App Group: `<team id>.com.eiluviann.vaultday`. */
const TEAM_APP_GROUP_PATTERN = /^[A-Z0-9]{10}\.com\.eiluviann\.vaultday$/;
const TOKEN_FILE_NAME = 'token';
const PORT_FILE_NAME = 'port';
const REQUEST_TIMEOUT_MILLISECONDS = 5000;
const HTTP_UNAUTHORIZED = 401;
const HELLO_ROUTE = '/v1/hello';
const CHALLENGE_HEADER = 'x-vaultday-challenge';
const SERVER_PROOF_CONTEXT = 'vaultday-server-proof\n';
const CHALLENGE_BYTE_COUNT = 32;
const FAILURE_KIND_BY_STATUS: ReadonlyMap<number, FailureKind> = new Map([
	[400, 'invalid-input'],
	[404, 'not-found'],
	[409, 'conflict'],
	[413, 'invalid-input'],
]);
const CONNECTION_REFUSED_CODES: ReadonlySet<string> = new Set([
	'ECONNREFUSED',
	'ECONNRESET',
	'EHOSTUNREACH',
	'UND_ERR_SOCKET',
]);

/**
 * The folders the app may write its API files into, most trusted first:
 * `VAULTDAY_API_DIR` alone when set (the tests); otherwise the shipped App
 * Group's `API` folder, a Personal Team build's
 * (`<team id>.com.eiluviann.vaultday`), then
 * `~/Library/Application Support/Vaultday/API`, where a build signed without
 * an App Group writes them (see `docs/signing.md`).
 */
export function apiDirectoryCandidatesFor(
	environment: Environment,
	homeDirectory: string = homedir(),
	groupContainerNames: readonly string[] = groupContainerNamesIn(homeDirectory),
): readonly string[] {
	const override = environment[API_DIRECTORY_ENVIRONMENT_KEY]?.trim();
	if (override) return [override];
	const library = path.join(homeDirectory, 'Library');
	const teamAppGroups = groupContainerNames
		.filter((name) => TEAM_APP_GROUP_PATTERN.test(name))
		.toSorted();
	return [APP_GROUP_IDENTIFIER, ...teamAppGroups]
		.map((group) => path.join(library, 'Group Containers', group, 'API'))
		.concat(path.join(library, 'Application Support', 'Vaultday', 'API'));
}

/**
 * Finds the running app: the first folder, in the order of
 * {@link apiDirectoryCandidatesFor}, that holds a token and a port and whose
 * server proves it holds that token. Trust order, not file age, decides, so
 * port files planted in the unprotected Application Support folder never
 * outrank a running App Group build; a port a crashed build left behind is
 * skipped because nothing answers on it.
 *
 * @returns the connection, or `app-not-running` when no folder holds both
 *   files or none answers, or the first other failure a listening server
 *   gave (a failed proof reads as `refused-token`).
 */
export async function readApiConnection(
	environment: Environment,
	homeDirectory: string = homedir(),
	proveServer: ServerProver = verifyServerHoldsToken,
): Promise<Result<ApiConnection, Failure>> {
	const candidates = apiDirectoryCandidatesFor(environment, homeDirectory);
	const found = await Promise.all(candidates.map(readConnectionIn));
	const connections = found.filter((connection) => connection !== undefined);
	const failures: Failure[] = [];
	for (const connection of connections) {
		// Sequential on purpose: the most trusted folder that answers wins.
		const proof = await proveServer(connection);
		if (proof.ok) return ok(connection);
		failures.push(proof.error);
	}
	return err(
		failures.find((failure) => failure.kind !== 'app-not-running') ??
			notRunning(),
	);
}

/**
 * Sends one request to the app's localhost API and returns the parsed JSON
 * body. The app first proves it holds the token (an HMAC of a fresh
 * challenge), so a program squatting on a stale port never receives the
 * token. A closed socket reads as not running; a failed proof or a 401 as a
 * refused token; any other refusal carries the app's own message.
 */
export async function callApi(
	connection: ApiConnection,
	method: ApiMethod,
	route: string,
	body?: unknown,
): Promise<Result<unknown, Failure>> {
	const verification = await verifyServerHoldsToken(connection);
	if (!verification.ok) return verification;
	const sent = await send(`${connection.baseUrl}${route}`, {
		method,
		headers: requestHeaders(connection.token, body !== undefined),
		body: body === undefined ? null : JSON.stringify(body),
	});
	if (!sent.ok) return sent;
	const response = sent.value;
	if (response.status === HTTP_UNAUTHORIZED) return err(refusedToken());
	const parsed = await parseJsonBody(response);
	if (response.ok) return ok(parsed);
	return err({
		kind: FAILURE_KIND_BY_STATUS.get(response.status) ?? 'internal',
		message: errorMessageOf(parsed) ?? `Vaultday answered ${response.status}`,
	});
}

/** The HMAC-SHA256, keyed by the token, the app answers `/v1/hello` with; hex. */
export function serverProofFor(token: string, challenge: string): string {
	return createHmac('sha256', token)
		.update(`${SERVER_PROOF_CONTEXT}${challenge}`)
		.digest('hex');
}

async function verifyServerHoldsToken(
	connection: ApiConnection,
): Promise<Result<undefined, Failure>> {
	const challenge = randomBytes(CHALLENGE_BYTE_COUNT).toString('hex');
	const sent = await send(`${connection.baseUrl}${HELLO_ROUTE}`, {
		method: 'GET',
		headers: { [CHALLENGE_HEADER]: challenge },
	});
	if (!sent.ok) return sent;
	const proof = sent.value.ok
		? proofOf(await parseJsonBody(sent.value))
		: undefined;
	const isProven =
		proof !== undefined &&
		isExpectedProof(proof, serverProofFor(connection.token, challenge));
	return isProven ? ok(undefined) : err(refusedToken());
}

async function send(
	url: string,
	init: RequestInit,
): Promise<Result<Response, Failure>> {
	try {
		return ok(
			await fetch(url, {
				...init,
				signal: AbortSignal.timeout(REQUEST_TIMEOUT_MILLISECONDS),
			}),
		);
	} catch (thrown) {
		return err(
			isConnectionRefused(thrown)
				? notRunning()
				: { kind: 'internal', message: describeThrown(thrown) },
		);
	}
}

function requestHeaders(token: string, hasBody: boolean): Headers {
	const headers = new Headers({ authorization: `Bearer ${token}` });
	if (hasBody) headers.set('content-type', 'application/json');
	return headers;
}

async function readTrimmedFile(filePath: string): Promise<string | undefined> {
	try {
		return (await readFile(filePath, 'utf8')).trim();
	} catch {
		return undefined;
	}
}

async function parseJsonBody(response: Response): Promise<unknown> {
	const text = await response.text();
	if (text === '') return undefined;
	try {
		const parsed: unknown = JSON.parse(text);
		return parsed;
	} catch {
		return undefined;
	}
}

function proofOf(body: unknown): string | undefined {
	if (typeof body !== 'object' || body === null) return undefined;
	const proof: unknown = Reflect.get(body, 'proof');
	return typeof proof === 'string' ? proof : undefined;
}

function isExpectedProof(presented: string, expected: string): boolean {
	const presentedBytes = Buffer.from(presented, 'utf8');
	const expectedBytes = Buffer.from(expected, 'utf8');
	return (
		presentedBytes.length === expectedBytes.length &&
		timingSafeEqual(presentedBytes, expectedBytes)
	);
}

function errorMessageOf(body: unknown): string | undefined {
	if (typeof body !== 'object' || body === null) return undefined;
	const message: unknown = Reflect.get(body, 'error');
	return typeof message === 'string' && message !== '' ? message : undefined;
}

/** fetch wraps the socket error in `cause`; a refused or reset connection means no app is listening. */
function isConnectionRefused(thrown: unknown): boolean {
	const cause: unknown =
		thrown instanceof Error ? Reflect.get(thrown, 'cause') : undefined;
	const code: unknown =
		typeof cause === 'object' && cause !== null
			? Reflect.get(cause, 'code')
			: undefined;
	return typeof code === 'string' && CONNECTION_REFUSED_CODES.has(code);
}

function describeThrown(thrown: unknown): string {
	return thrown instanceof Error ? thrown.message : String(thrown);
}

function refusedToken(): Failure {
	return { kind: 'refused-token', message: REFUSED_TOKEN_MESSAGE };
}

function notRunning(): Failure {
	return { kind: 'app-not-running', message: NOT_RUNNING_MESSAGE };
}

async function readConnectionIn(
	directory: string,
): Promise<ApiConnection | undefined> {
	const token = await readTrimmedFile(path.join(directory, TOKEN_FILE_NAME));
	const portText = await readTrimmedFile(path.join(directory, PORT_FILE_NAME));
	const port = Number(portText);
	const isUsablePort = Number.isInteger(port) && port > 0 && port < 65_536;
	if (token === undefined || token === '' || !isUsablePort) return undefined;
	return { baseUrl: `http://127.0.0.1:${port}`, token };
}

function groupContainerNamesIn(homeDirectory: string): readonly string[] {
	try {
		return readdirSync(path.join(homeDirectory, 'Library', 'Group Containers'));
	} catch {
		return [];
	}
}
