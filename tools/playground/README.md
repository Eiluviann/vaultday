# playground

Generates the synthetic playground Vaultday is tried out on: two workspace roots and a command that opens the installed app on them. Everything derives from `core/fixtures` and is synthetic (generic names such as WEB-142, a tracker at `tracker.example.test`).

```bash
scripts/playground.sh                     # writes ~/Vaultday-playground, or resets it
scripts/playground.sh --out /tmp/pg --today 2026-10-24 --now 11:00
pnpm --filter @vaultday/playground test
```

## What it writes

```
~/Vaultday-playground/
  Mock Studio/                       work root (.vaultday/settings.json, .obsidian/)
  Personal/                          life root (.vaultday/settings.json, .obsidian/)
  Open Vaultday Playground.command   opens Finder and ~/Applications/Vaultday.app, prints the steps
```

- **Mock Studio** (work, copper): the mock vault's Website and Acme portal hubs with WEB-142, WEB-157, ACME-9 and ACME-12, plus WEB-163 (estimated, in progress), WEB-171, ACME-15 (due this Friday, blocked on credentials) and ACME-18 (no estimate). Day logs for the previous week (Monday to Thursday; Friday is leave) and the week holding today, with sessions, decisions, blockers and status lines: 23 sessions, 30 h 35 min in the focus week. Hourly billing rounded to 15 minutes and a Monday 12:00 log deadline, so Hours and the week close-out have a week to show. Pending Input items wait between 3 and 24 days.
- **Personal** (life, green): two projects and an admin list with scheduled, due and overdue tasks, groceries, the logs of yesterday and today, and `Leave.md` (the leave source Mock Studio names).

Dates are offsets from the Monday of the week holding today, so a reset on any day gives the same week shape: a weekend run shows the whole week, a weekday run the days so far, with no log line after the current time and no task done or created after today. The fixtures' dates move by the same offset, and their provenance markers get the hash of their new text.

## Resetting

Running the generator again replaces every note in both roots and keeps `.vaultday/sync` (the app's workspace id and change files) and `.obsidian/`, so the app keeps both workspaces registered. A root without the `.vaultday-playground` marker is refused, so the generator never clears a folder it did not write. An earlier single-root playground (`~/Vaultday-playground/.vaultday`) is moved whole to `~/Vaultday-playground-archive/<stamp>/`.

## Why two clicks remain

The app is sandboxed: it reads a folder outside its container only through a security-scoped bookmark, and the sandbox grants one only for a folder the user picks in the open panel. `VAULTDAY_DEBUG_WORKSPACE` (debug builds) cannot stand in for that: it adds a single folder, only when the app has no workspace, and the sandbox still refuses a folder nobody picked. So each root is imported once on the setup's Workspaces step (Add, Import a workspace, pick the folder, Done); both already hold their settings, so the Configure sheet needs no input.
