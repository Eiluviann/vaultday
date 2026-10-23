export type Result<T, E = string> =
	| { ok: true; value: T }
	| { ok: false; error: E };

export function ok<T>(value: T): Result<T, never> {
	return { ok: true, value };
}

export function err<E>(error: E): Result<never, E> {
	return { ok: false, error };
}

/** Converts every item, stopping at the first failure. */
export function collectResults<T, U>(
	items: readonly T[],
	convert: (item: T) => Result<U>,
): Result<U[]> {
	const values: U[] = [];
	for (const item of items) {
		const result = convert(item);
		if (!result.ok) return result;
		values.push(result.value);
	}
	return ok(values);
}

/** Prefixes the error message of a failed result with where it happened. */
export function withErrorContext<T>(
	context: string,
	result: Result<T>,
): Result<T> {
	return result.ok ? result : err(`${context}: ${result.error}`);
}
