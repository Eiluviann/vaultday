export type ParsedArguments =
	| {
			readonly command: 'today';
			readonly workspacePath: string;
			readonly date?: string | undefined;
			readonly shouldPrintTiming: boolean;
	  }
	| { readonly command: 'help' };

export const USAGE =
	'Usage: vaultday today <workspace path> [--date YYYY-MM-DD] [--timing]';

/** Parses `argv` after the executable and script; returns `help` for anything it does not understand. */
export function parseArguments(argv: readonly string[]): ParsedArguments {
	const [command, workspacePath, ...rest] = argv;
	if (command !== 'today' || workspacePath === undefined)
		return { command: 'help' };
	let date: string | undefined;
	let shouldPrintTiming = false;
	for (let index = 0; index < rest.length; index += 1) {
		const flag = rest[index];
		if (flag === '--date') {
			date = rest[index + 1];
			if (date === undefined || date.startsWith('--'))
				return { command: 'help' };
			index += 1;
		} else if (flag === '--timing') {
			shouldPrintTiming = true;
		} else {
			return { command: 'help' };
		}
	}
	return { command: 'today', workspacePath, date, shouldPrintTiming };
}
