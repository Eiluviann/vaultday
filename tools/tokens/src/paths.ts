import path from 'node:path';
import { fileURLToPath } from 'node:url';

export const REPOSITORY_ROOT = path.resolve(
	path.dirname(fileURLToPath(import.meta.url)),
	'..',
	'..',
	'..',
);

export const DEFAULT_TOKENS_PATH = 'design/tokens.json';
export const DEFAULT_SWIFT_OUTPUT_PATH = 'apps/apple/Vaultday/Tokens.swift';
