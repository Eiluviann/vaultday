# provenance-core

A vendored copy of the AI-provenance engine that the Claude Code vault hook runs (`~/.claude/hooks/lib/obsidian-ai-provenance-core.mjs`) together with its tests. `vault-migrate` loads it today (as the `@vaultday/provenance-core` workspace dependency) to refresh provenance fingerprints, and the `vaultday` CLI will use it to write `%% ai:… %%` markers on model-originated text. Vendoring keeps the repo buildable on CI, where the hook is not installed.

The engine stays plain JavaScript because the hook runs it without a build step. `vendor/` is excluded from Biome for that reason.

- `pnpm test` runs the vendored tests, then the parity check.
- `pnpm parity` fails when the vendored files differ from the live hook (skipped where no hook is installed).
- `pnpm sync` copies the live hook files over the vendored ones; commit the result.
