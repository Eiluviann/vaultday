import path from 'node:path';
import { fileURLToPath } from 'node:url';

export const REPOSITORY_ROOT = path.resolve(
	path.dirname(fileURLToPath(import.meta.url)),
	'..',
	'..',
	'..',
);

export const KEYMAP_PATH = 'apps/apple/Keymap.yaml';
export const KEYMAP_DOC_PATH = 'apps/apple/KEYMAP.md';
export const KEYMAP_NOTES_PATH = 'apps/apple/KEYMAP.notes.md';
