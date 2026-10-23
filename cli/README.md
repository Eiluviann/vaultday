# @vaultday/cli

The `vaultday` command line and a stdio MCP server over the Rust core
(`@vaultday/core-node`, the napi-rs binding). It touches **vault files only**:
it never reads or writes `.vaultday/sync/` and does not depend on the app.

The workspace is the deepest `.vaultday/` at or above the working directory.
Outside a workspace every command prints why and exits 1; usage errors exit 2.

## Commands

```
vaultday today [path] [--date YYYY-MM-DD] [--json]
vaultday log <session|done|decision|blocker|status|note> <text>
        [--story <number>] [--task <id>] [--date YYYY-MM-DD] [--time HH:MM] [--ai|--human] [--json]
vaultday report <story number|client> --from YYYY-MM-DD --to YYYY-MM-DD
vaultday task add <text> --story <number> [--section next|pending] [--assigned-to <who>] [--ai|--human] [--json]
vaultday task done <id> [--json]
vaultday mcp
```

- `today`: stories with open Next Steps and Pending Input tasks, the day's log lines and the waiting-on items with their age.
- `log` appends `- HH:MM <kind> · <text> [story:: …] [kind:: …] [task:: …]` to `Log/YYYY-MM-DD.md` (creating the file with the convention's header).
- `report` prints one JSON bundle: per story the frontmatter (each key an array of values), Next Steps and Pending Input state, the log lines in range, and an `authorship` summary computed with the vendored provenance core. A story number reports that story; a client (a project hub's key, title or folder name, or the workspace name) reports every story under it.
- `task add` writes `- [ ] text [id:: task-N] [created:: …]` under `## Next Steps` (default) or `## Pending Input`; `task done` ticks `[x]` and adds `[completion:: …]`, changing that one line.

### Model-written text

Text a model wrote is marked; fact lines are not.

- **Marking is on** with `--ai`, or when `AI_PROVENANCE_AUTHOR` is set (the variable the vault's provenance hook reads); `--human` turns it off. The author is `AI_PROVENANCE_AUTHOR`, else `ANTHROPIC_MODEL`, else `claude-unknown`.
- A **log line** names its author in `[agent:: <author>]`, the attribution the provenance core accepts for day-log lines (the `Log/` folder is exempt from block markers).
- A **task line** is wrapped in `%% ai:begin <author> · <date> · <hash> %%` … `%% ai:end %%`, with the hash computed by `@vaultday/provenance-core` so the block reads as intact.

## MCP server

Tools: `today`, `log`, `report`, `task_add`, `task_done`, with JSON schemas. Text sent through the tools is model text unless `isModelText` is `false`. The workspace comes from the server's working directory, which Claude Code sets to the session's folder.

Register the **built** server (never `tsx`):

```bash
pnpm --filter @vaultday/core-node build && pnpm --filter @vaultday/cli build
claude mcp add vaultday -- node <repo>/cli/dist/mcp.js
```

## Build, test, pack

```bash
pnpm --filter @vaultday/core-node build   # the napi binary for this machine
pnpm --filter @vaultday/cli build         # dist/main.js and dist/mcp.js (esbuild; everything but the binding is inlined)
pnpm --filter @vaultday/cli test          # Vitest, against a temp copy of core/fixtures/mock-vault
scripts/build-node.sh                     # tarballs in dist-node/
```

`scripts/build-node.sh` packs `@vaultday/core-node` (with the `.node` binary for the host platform) and `@vaultday/cli`. Install both in one command so the CLI's dependency on the binding is met by the tarball:

```bash
npm install -g ./dist-node/vaultday-core-node-*.tgz ./dist-node/vaultday-cli-*.tgz
claude mcp add vaultday -- node "$(npm root -g)/@vaultday/cli/dist/mcp.js"
```

(`pnpm add -g` of the two tarballs fails because pnpm looks `@vaultday/core-node` up in the registry; npm resolves it from the other tarball.)
