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

- **Mock Studio** (work, copper): the mock vault's Website and Acme portal hubs with WEB-142, WEB-157, ACME-9 and ACME-12, plus WEB-163 (estimated, in progress), WEB-171, ACME-15 (due this Friday, blocked on credentials) and ACME-18 (no estimate). Day logs for the previous week (Monday to Thursday; Friday is leave) and the week holding today, with sessions, decisions, blockers and status lines: 23 sessions, 30 h 35 min in the focus week on a weekend run. Hourly billing rounded to 15 minutes and a Monday 12:00 log deadline, so Hours and the week close-out have a week to show. Pending Input items wait from a few days to about three weeks (0 to 20 days on a Monday, 4 to 25 on a Saturday).
- **Personal** (life, green): two projects and an admin list with scheduled, due and overdue tasks, groceries, the logs of yesterday and today, and `Leave.md` (the leave source Mock Studio names).

Mock Studio's dates are offsets from the Monday of the week holding today, so a reset on any day gives the same week shape: a weekend run shows the whole week, a weekday run the days so far. Personal's dates are offsets from today, so its due dates always read "tomorrow" or "in a week". Every note is held to the moment of the run: no log line after the current time (a session still running is not written), no task done or created after today (a task whose done line comes later today is still open), no dated `## Notes` line after today, and no provenance marker dated after today. The fixtures' dates move with Mock Studio's, and their provenance markers get the hash of their new text (a verification token stays as written). `Leave.md` is the one note dated ahead on purpose.

## Resetting

Running the generator again replaces every note in both roots and keeps `.vaultday/sync` (the app's workspace id and change files) and `.obsidian/`, so the app keeps both workspaces registered. The generator refuses, before changing anything:

- a root without the `.vaultday-playground` marker (it never clears a folder it did not write), and a root that is a file;
- a playground folder that is or lies in a workspace root (a folder holding `.vaultday`), so a real workspace given as `--out` is left as it is;
- a playground folder whose real path (symlinks resolved, case ignored) lies in `~/Vaults`, `~/Library` or a vaultday checkout, fixtures included.

The marker is written first in each root, so a run that stops part way leaves roots the next run may reset; such a stop is reported, not thrown.

## Why two clicks remain

The app is sandboxed: it reads a folder outside its container only through a security-scoped bookmark, and the sandbox grants one only for a folder the user picks in the open panel. `VAULTDAY_DEBUG_WORKSPACE` (debug builds) cannot stand in for that: it adds a single folder, only when the app has no workspace, and the sandbox still refuses a folder nobody picked. So each root is imported once on the setup's Workspaces step (Add, Import a workspace, pick the folder, Done); both already hold their settings, so the Configure sheet needs no input.
