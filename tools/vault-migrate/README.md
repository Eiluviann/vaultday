# vault-migrate

One-off migration of the Obsidian vaults to note system v1 (vault conventions v2). Runs against the preview vaults under `~/Vaults/Preview`; `--apply` refuses any other path unless `--allow-live` is passed, and always refuses iCloud copies.

```bash
pnpm install
pnpm test && pnpm typecheck && pnpm lint
pnpm start -- --vault ~/Vaults/Preview/Work --dry-run --birthtimes state/Work.birthtimes.tsv
pnpm start -- --vault ~/Vaults/Preview/Work --apply   --birthtimes state/Work.birthtimes.tsv
pnpm start -- --vault ~/Vaults/Preview/Work --verify-links
pnpm start -- --vault ~/Vaults/Preview/Work --tick task-12 --dry-run
```

Rules (`--rules frontmatter,tasks,rename,queries,moves`, default all): frontmatter key renames and catalogue keys, task ids and field normalisation, `STRY… Short title` folder renames, dataview/tasks query rewrites, layout moves from `config/<profile>.moves.json`. Wikilinks and quoted paths follow renames and moves. Units whose only changes are mechanical get their provenance fingerprint refreshed through the hook core at `~/.claude/hooks/lib/obsidian-ai-provenance-core.mjs`; everything after the hash on a marker line is kept.

`state/` (gitignored) holds birthtime manifests, before/after `check` outputs, dry-run diffs and apply manifests. Task-id counters live in `~/.local/state/vaultday/ids/`.
