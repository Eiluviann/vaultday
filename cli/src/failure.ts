/**
 * Why a command did not run. The kinds mirror the codes of the native binding,
 * plus the CLI's own: usage and internal, and the two answers of the app's
 * localhost API that are not about the request (not running, refused token).
 */
export type FailureKind =
	| 'not-a-workspace'
	| 'inactive-workspace'
	| 'invalid-settings'
	| 'invalid-date'
	| 'invalid-input'
	| 'not-found'
	| 'conflict'
	| 'file-access'
	| 'usage'
	| 'internal'
	| 'app-not-running'
	| 'refused-token';

export type Failure = {
	readonly kind: FailureKind;
	readonly message: string;
};

const FAILURE_KINDS: readonly FailureKind[] = [
	'not-a-workspace',
	'inactive-workspace',
	'invalid-settings',
	'invalid-date',
	'invalid-input',
	'not-found',
	'conflict',
	'file-access',
	'usage',
	'internal',
	'app-not-running',
	'refused-token',
];

export function usageFailure(message: string): Failure {
	return { kind: 'usage', message };
}

/** Reads the `code` the native binding puts on the errors it throws. */
export function failureFromThrown(thrown: unknown): Failure {
	if (!(thrown instanceof Error)) {
		return { kind: 'internal', message: String(thrown) };
	}
	const code: unknown = Reflect.get(thrown, 'code');
	const kind = FAILURE_KINDS.find((candidate) => candidate === code);
	return { kind: kind ?? 'internal', message: thrown.message };
}
