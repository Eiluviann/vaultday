/** Why a command did not run. The kinds mirror the codes of the native binding, plus two of the CLI's own. */
export type FailureKind =
	| 'not-a-workspace'
	| 'invalid-settings'
	| 'invalid-date'
	| 'invalid-input'
	| 'not-found'
	| 'conflict'
	| 'file-access'
	| 'usage'
	| 'internal';

export type Failure = {
	readonly kind: FailureKind;
	readonly message: string;
};

const FAILURE_KINDS: readonly FailureKind[] = [
	'not-a-workspace',
	'invalid-settings',
	'invalid-date',
	'invalid-input',
	'not-found',
	'conflict',
	'file-access',
	'usage',
	'internal',
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
