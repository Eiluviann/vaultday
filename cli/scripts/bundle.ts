/**
 * Bundles the CLI and the MCP server into `dist/main.js` and `dist/mcp.js`.
 * Everything pure JavaScript (the MCP SDK, zod, the vendored provenance core)
 * is inlined, so a packed `@vaultday/cli` needs only `@vaultday/core-node`,
 * whose native binary cannot be bundled.
 */
import { chmodSync, rmSync } from 'node:fs';
import { build } from 'esbuild';

const OUTPUT_DIRECTORY = 'dist';
// The bundled CommonJS dependencies of the MCP SDK call `require`.
const REQUIRE_SHIM =
	"import { createRequire as createRequireForBundle } from 'node:module'; const require = createRequireForBundle(import.meta.url);";

rmSync(OUTPUT_DIRECTORY, { recursive: true, force: true });
await build({
	entryPoints: { main: 'src/main.ts', mcp: 'src/mcp.ts' },
	outdir: OUTPUT_DIRECTORY,
	bundle: true,
	platform: 'node',
	target: 'node22',
	format: 'esm',
	external: ['@vaultday/core-node'],
	banner: { js: REQUIRE_SHIM },
	sourcemap: false,
	logLevel: 'info',
});
for (const entry of ['main.js', 'mcp.js']) {
	chmodSync(`${OUTPUT_DIRECTORY}/${entry}`, 0o755);
}
