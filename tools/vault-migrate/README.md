# vault-migrate

One-off migration of the Obsidian vaults to note system v1 (vault conventions v2). Runs against the preview vaults under `~/Vaults/Preview`; `--apply` refuses any other path unless `--allow-live` is passed, and always refuses iCloud copies.

```bash
pnpm install
pnpm test && pnpm typecheck && pnpm lint
pnpm start -- --vault ~/Vaults/Preview/Work --dry-run --birthtimes state/Work.birthtimes.tsv
pnpm start -- --vault ~/Vaults/Preview/Work --apply   --birthtimes state/Work.birthtimes.tsv
pnpm start -- --vault ~/Vaults/Preview/Work --verify-links
pnpm start -- --vault ~/Vaults/Preview/Work --tick task-12 --dry-run
pnpm start -- lint-log --vault ~/Vaults/Preview/Work
```

Rules (`--rules frontmatter,tasks,rename,queries,moves`, default all): frontmatter key renames and catalogue keys, task ids and field normalisation, `STRY… Short title` folder renames, dataview/tasks query rewrites, layout moves from `<profile>.moves.json`. Wikilinks and quoted paths follow renames and moves. Units whose only changes are mechanical get their provenance fingerprint refreshed through the provenance core vendored in `tools/provenance-core` (kept identical to the live hook by its parity check; `--core <path>` points elsewhere); everything after the hash on a marker line is kept.

`lint-log` validates every `Log/*.md` against the day-log grammar (dated file name and frontmatter, `## Log` list items with `HH:MM`, exactly one `kind`, at most one existing `story` or `project`, existing `task`, single-word `agent`, no nesting, no `|`) and exits 1 on findings.

## Profiles

Profiles hold vault-specific data (hubs, type guesses, moves), so they live outside the repo, in `$XDG_CONFIG_HOME/vault-tools/vault-migrate` (default `~/.config/vault-tools/vault-migrate`). `--config-dir <path>` overrides it. For a vault folder `Work` the tool reads `work.json` and `work.moves.json` (the vault folder name, lowercased; `--profile <name>` picks another). A missing profile runs with an empty profile and a warning. `tools/vault-migrate/config/` is gitignored so a local copy cannot be committed.

Example `work.json`:

```json
{
	"vaultKind": "work",
	"togglProject": "Alpha Co",
	"hubs": { "Alpha": "ALPHA" },
	"typeGuesses": [
		{ "path": "Projects/Alpha/Notes.md", "type": "note", "kind": "reference" }
	],
	"skipNotes": []
}
```

`vaultKind` is `work` or `personal`.

`state/` (gitignored) holds birthtime manifests, before/after `check` outputs, dry-run diffs and apply manifests. Task-id counters live in `~/.local/state/vaultday/ids/`.
