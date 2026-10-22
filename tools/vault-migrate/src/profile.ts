import { readFileSync } from 'node:fs';
import { z } from 'zod';
import { err, ok, type Result } from './result.ts';

const typeGuessSchema = z.object({
	path: z.string().min(1),
	type: z.string().min(1),
	kind: z.string().optional(),
	status: z.string().optional(),
	story: z.string().optional(),
	project: z.string().optional(),
	aliases: z.array(z.string()).optional(),
});

const profileSchema = z.object({
	vaultKind: z.enum(['work', 'personal']),
	togglProject: z.string().optional(),
	hubs: z.record(z.string(), z.string()).default({}),
	typeGuesses: z.array(typeGuessSchema).default([]),
	skipNotes: z.array(z.string()).default([]),
});

const moveSchema = z.union([
	z.object({
		from: z.string().min(1),
		to: z.string().min(1),
		toVault: z.string().optional(),
		except: z.array(z.string()).optional(),
	}),
	z.object({ delete: z.string().min(1) }),
]);

export type TypeGuess = z.infer<typeof typeGuessSchema>;
export type Profile = z.infer<typeof profileSchema>;
export type MoveSpec = z.infer<typeof moveSchema>;

export function loadProfile(filePath: string): Result<Profile> {
	return parseJsonFile(filePath, profileSchema);
}

export function loadMoves(filePath: string): Result<MoveSpec[]> {
	return parseJsonFile(filePath, z.array(moveSchema));
}

export function emptyProfile(vaultKind: Profile['vaultKind']): Profile {
	return { vaultKind, hubs: {}, typeGuesses: [], skipNotes: [] };
}

function parseJsonFile<T>(filePath: string, schema: z.ZodType<T>): Result<T> {
	let raw: unknown;
	try {
		raw = JSON.parse(readFileSync(filePath, 'utf8'));
	} catch (error) {
		return err(
			`cannot read ${filePath}: ${error instanceof Error ? error.message : String(error)}`,
		);
	}
	const parsed = schema.safeParse(raw);
	return parsed.success
		? ok(parsed.data)
		: err(`${filePath}: ${parsed.error.message}`);
}
