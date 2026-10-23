import { readFileSync } from 'node:fs';
import { err, ok, type Result } from './result.ts';
import { generateTokensSwift } from './swift-file.ts';
import { parseTokenFile } from './token-schema.ts';

/** Reads and validates the token JSON at `tokensPath`, then renders `Tokens.swift`. */
export function generateTokensSwiftFromFile(
	tokensPath: string,
): Result<string> {
	const text = readTextFile(tokensPath);
	if (!text.ok) return text;
	const json = parseJson(text.value, tokensPath);
	if (!json.ok) return json;
	const tokenFile = parseTokenFile(json.value);
	if (!tokenFile.ok) return err(`${tokensPath}: ${tokenFile.error}`);
	return generateTokensSwift(tokenFile.value);
}

/** Reads a UTF-8 file, or returns the reason it cannot be read. */
export function readTextFile(filePath: string): Result<string> {
	try {
		return ok(readFileSync(filePath, 'utf8'));
	} catch (error) {
		return err(`cannot read ${filePath}: ${describeError(error)}`);
	}
}

function parseJson(text: string, sourcePath: string): Result<unknown> {
	try {
		return ok(JSON.parse(text));
	} catch (error) {
		return err(`${sourcePath} is not valid JSON: ${describeError(error)}`);
	}
}

function describeError(error: unknown): string {
	return error instanceof Error ? error.message : String(error);
}
