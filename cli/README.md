# @vaultday/cli

The `vaultday` command line and a stdio MCP server over the Rust core
(`@vaultday/core-node`, the napi-rs binding). `today`, `log`, `report` and
`task` touch **vault files only**: they never read or write `.vaultday/sync/`
and do not depend on the app. `timer` and `status` are the exception: they
talk **only** to the running Vaultday app over its localhost API, so the timer
has one owner (see [Timer and status](#timer-and-status)).

The workspace is the nearest `.vaultday/` at or above the working directory,
taken as the shell names it, so a folder linked into a workspace root resolves
to that workspace. Started inside a linked folder's real path, the CLI finds no
workspace, since it never reads the app's list of links. Outside a workspace,
or in one deactivated in the app, every command prints why and exits 1; usage
errors exit 2.

## Commands

```
vaultday today [path] [--date YYYY-MM-DD] [--json]
vaultday log <session|done|decision|blocker|status|note> <text>
        [--story <number>] [--task <id>] [--date YYYY-MM-DD] [--time HH:MM] [--ai|--human] [--json]
vaultday report <story number|client> --from YYYY-MM-DD --to YYYY-MM-DD
vaultday task add <text> --story <number> [--section next|pending] [--assigned-to <who>] [--ai|--human] [--json]
vaultday task done <id> [--json]
vaultday timer start <task-id|text> [--ago <minutes>] [--json]
vaultday timer stop [--json]
vaultday timer continue [--json]
vaultday status [--format text|json|sketchybar] [--json]
vaultday mcp
```

- `today`: stories with open Next Steps and Pending Input tasks, the day's log lines and the waiting-on items with their age.
- `log` appends `- HH:MM <kind> · <text> [story:: …] [kind:: …] [task:: …]` to `Log/YYYY-MM-DD.md` (creating the file with the convention's header).
- `report` prints one JSON bundle: per story the frontmatter (each key an array of values), Next Steps and Pending Input state, the log lines in range, and an `authorship` summary computed with the vendored provenance core. A story number reports that story; a client (a project hub's key, title or folder name, or the workspace name) reports every story under it. The keys come in the core's order, the same bundle the app hands out: `subject`, `subjectKind`, `from`, `to`, `billing` (the workspace's billing model, `null` until one is set), `stories`, `log`, `sessions` (the `kind:: session` lines: count, those without a duration, minutes logged, per story), `hours` (by billing model: logged, hourly billable and non-billable, fixed price against the quote, or FTE logged hours only) and `authorship`, the one key the CLI fills in.
- `task add` writes `- [ ] text [id:: task-N] [created:: …]` under `## Next Steps` (default) or `## Pending Input`; `task done` ticks `[x]` and adds `[completion:: …]`, changing that one line.

### Model-written text

Text a model wrote is marked; fact lines are not.

- **Marking is on** with `--ai`, or when `AI_PROVENANCE_AUTHOR` is set (the variable the vault's provenance hook reads); `--human` turns it off. The author is `AI_PROVENANCE_AUTHOR`, else `ANTHROPIC_MODEL`, else `claude-unknown`.
- A **log line** names its author in `[agent:: <author>]`, the attribution the provenance core accepts for day-log lines (the `Log/` folder is exempt from block markers).
- A **task line** is wrapped in `%% ai:begin <author> · <date> · <hash> %%` … `%% ai:end %%`, with the hash computed by `@vaultday/provenance-core` so the block reads as intact.

## Timer and status

`vaultday timer` and `vaultday status` need the app running; they never touch
the vault or the synced session document themselves.

- `timer start <task-id|text>` starts the timer on a task id of the day's list (or any `task-N`), a story number (`WEB-142`, a session on the story), or the first open task of the day whose text holds all the words. `--ago 10` is "Start minutes ago": the session began ten minutes before now. A running session is stopped and logged first, as on the Now card.
- `timer stop` stops the running or paused session. The app writes its line to the day log through the core's writer, `- 09:30 session · <task text> · 47m [story:: …] [kind:: session] [id:: ses-…] [task:: …]`, once per session: a day log that already holds the session's id is left alone, whichever surface stopped it. The command prints that line.
- `timer continue` is "Continue latest": a new session on the task the last timer ran on.
- `status` prints the running timer and the day's logged time; `--format json` (or `--json`) the app's JSON, `--format sketchybar` the lines below.

When the app is not running (no port file, or nothing listens) every one of
them exits 1 with `Vaultday is not running`; when the app answers 401 they exit
1 with `Vaultday refused the token`. Any other refusal carries the app's own
message (`No open task of today matches "lunch"`, `No timer is running`).

### How the CLI finds the app

At each start the app writes two files, mode 0600, in a 0700 folder of its App
Group container:

```
~/Library/Group Containers/group.com.eiluviann.vaultday/API/token   # 64 hex digits
~/Library/Group Containers/group.com.eiluviann.vaultday/API/port    # e.g. 47390
```

A free Personal Team build writes them to its team-prefixed App Group,
`~/Library/Group Containers/<team id>.com.eiluviann.vaultday/API/`, and a
build signed without an App Group (ad hoc, or the Personal Team fallback; see
[docs/signing.md](../docs/signing.md)) to
`~/Library/Application Support/Vaultday/API/`. The CLI tries the shipped App
Group folder first, then any team-prefixed one, then Application Support, and
uses the first that holds both files and whose server proves it holds that
token. A port a crashed build left behind is skipped because nothing answers
on it.

The token is random, made by the app on its first run and kept in the
Keychain (this Mac only, never synchronised, never in `settings.json` or the
vault); the file is rewritten from the Keychain at every launch. The port file
is removed when the app quits. `VAULTDAY_API_DIR=<folder>` points the CLI at
another folder holding `token` and `port`, and only there (the tests use a
fake app).

macOS may ask once whether your terminal may read another app's data the
first time the CLI reads the App Group container.

### The localhost API

The app serves HTTP/1.1 on `127.0.0.1` only (the listener binds the loopback
address and refuses any peer that is not loopback). Every request but
`/v1/hello` needs `Authorization: Bearer <token>`, compared in constant time; bodies over 64 KB
are refused with 413. Nothing is rate-limited. Answers are JSON; failures are
`{"error": "<message>"}`.

Before it sends the token, the CLI asks `GET /v1/hello` with a fresh
`X-Vaultday-Challenge: <64 hex digits>` and checks the answer `{"proof":
"<hex>"}` against HMAC-SHA256(key: token, message: `vaultday-server-proof\n`
followed by the challenge). A program squatting on a stale port, from another
account on the Mac for one, cannot make the proof and never sees the token; the
CLI then says `Vaultday refused the token`.

| Method and path | Body | Answer |
| --- | --- | --- |
| `GET /v1/hello` | | `{proof}`, no token needed |
| `GET /v1/status` | | status |
| `GET /v1/today` | | `{workspace, date, tasks: [{taskId, text, story, isDone, estimateMinutes}]}` |
| `POST /v1/timer/start` | `{"target": "task-5", "minutesAgo": 10}` | 201, status |
| `POST /v1/timer/stop` | | status with `logLine` |
| `POST /v1/timer/continue` | | 201, status |
| `POST /v1/tasks/<id>/done` | | `{taskId, isDone}` |

A status is `{workspace, timer: {sessionId, taskId, story, text, isPaused, startedAt, elapsedSeconds} | absent, day: {date, spentSeconds, loggedSeconds, workloadSeconds}}`.
Status codes: 400 bad body, 401 bad or missing token, 403 non-loopback peer,
404 no such route or no matching task, 405 wrong method, 409 a session already
runs or none does, 413 body too large, 503 no workspace open.

```bash
curl -s -H "Authorization: Bearer $(cat ~/Library/Group\ Containers/group.com.eiluviann.vaultday/API/token)" \
  "http://127.0.0.1:$(cat ~/Library/Group\ Containers/group.com.eiluviann.vaultday/API/port)/v1/status"
```

### sketchybar

`vaultday status --format sketchybar` prints one `key=value` per line, values
on one line and never quoted, in this order:

```
state=running            # running, paused or idle
elapsed=0:47:12          # H:MM:SS; empty when idle
task=task-5              # the running task's id; may be empty
story=WEB-142            # its story; may be empty
text=Write the end-to-end checkout test
label=0:47:12 task-5 · Write the end-to-end checkout test
logged=2h30              # logged today, short form
workload=8h              # the workload threshold
ring=2h30 / 8h           # logged / workload
workspace=Mock Studio
```

[`sketchybar/vaultday.sh`](sketchybar/vaultday.sh) is a sample item script: it
polls the command, reads the lines with `IFS='=' read -r key value` (never
`eval`), shows the label while a timer runs and the ring when idle, and hides
the item while the app is not running. A click stops the running timer, or
continues the latest one. Register it in `sketchybarrc`:

```bash
sketchybar --add item vaultday right \
           --set vaultday update_freq=2 \
                 script="$PLUGIN_DIR/vaultday.sh" \
                 click_script="$PLUGIN_DIR/vaultday.sh click"
```

Each poll starts Node; keep `update_freq` at 1 or above.

## MCP server

Tools: `today`, `log`, `report`, `task_add`, `task_done`, and the API-only `timer_start`, `timer_stop`, `timer_continue` and `status` (they answer "Vaultday is not running" without the app), with JSON schemas. Text sent through the tools is model text unless `isModelText` is `false`. The workspace comes from the server's working directory, which Claude Code sets to the session's folder.

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
