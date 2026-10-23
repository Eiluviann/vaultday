# Design debt

What PR 14 (polish, icon motion, deferred findings) could not settle in code, and every non-blocking review finding from the night's PRs (#14 to #21) that it did not take. Each line says where it stands and who or what it waits on.

## Left to the design tools

- **Six custom symbols.** The Teams mark, the lightning bolt, the running figure, the wand, the screen-share glyph and the hand are to be authored in the SF Symbols 7 app with draw annotations (start and end points, layer order). Until then the code uses the closest system symbol and says so with `// custom symbol pending` and `IconMark.isCustomSymbolPending`: `bubble.left.and.bubble.right` (Teams), `bolt` (captured), `figure.run` (chaser, and the Chasers lane on Triage all), `sparkles` (a decision a model wrote), `rectangle.on.rectangle` (screen-share in the header) and `hand.raised` (private tracked time). A system symbol without draw annotations falls back to the plain appear and disappear, as the design note allows.
- **The app icon.** The porthole icon still has to be built in Icon Composer (light, dark, tinted and clear layers from `design/icon/layers`); the app target has no icon asset yet, so the system's generic icon shows.
- **On-screen comparison of every board.** No GUI pass was made in this PR, as in the PRs before it. Every surface was built and unit-tested, but none was compared on screen with its board in the mockup (Icon motion, Home, All, Browse, Inbox, Story, Tracker, Menubar, Planning, the onboarding boards). The draw effects themselves (timing, layer order, the Reduce Motion crossfade) are only compile-verified.
- **The naming checks.** `tools/names/validate-names.py` and the by-eye checks of product naming (the retired name never appears; sentence case; no trailing period on single-sentence labels) were not rerun over the copy this PR added: the Triage lanes, the folded-subtasks line, the Obsidian notice, the Story block hover text, the hand's help, the menu-bar key labels and `KEYMAP.md`.
- **Motion tokens in the design system.** `design/tokens.json` now has a `motion` group (lift 180 ms, wash 150 ms, transition 150 ms) that the design-system artifact does not have yet. Add it there before the next export, or the export will drop it (the generator reads an export without it, so nothing breaks, but `Tokens.Motion` would vanish).
- **Mark colours.** `SourceMark` and `TypeMark` keep each source and type to one shape and one token (Mail in ink-secondary, Outlook and Teams in info, captures, blockers and chasers in caution, sessions, meetings and done in positive, model text in ai). The design note's own colour table was not available in this session; check the mapping against it.

## Icon motion the base version has no surface for yet

- **Switcher lock and bell.** The design's lock-to-unlock and bell-to-silenced replaces on a workspace row wait for workspace locking and silencing; the switcher rows have no symbol in the base version. `iconReplace` is ready for them.
- **Draw off before a card leaves.** A dismissed or snoozed signal's mark should draw off before the card goes. The card is removed by the model at once, so the mark leaves with it; doing it right needs the Inbox model to hold the card for the effect's length.
- **Health triangle and now-line clock.** The health footer's triangle and the now-line clock leave with their rows; neither draws off yet.
- **Guessed-value sparkle.** The sparkle that draws on over a guessed value belongs to the AI feature sets; the base version guesses nothing.
- **Watch complication.** No Watch app yet.
- **Variable draw on rings.** The logged and tracked rings are drawn shapes, not variable-value symbols, so they follow the same rule through `progressDraw` (draw between values after the first render, jump under Reduce Motion) without `symbolVariableValueMode(.draw)`. `iconVariableDraw` is there for a symbol-based ring.

## Deferred findings answered by design rather than code

- **`}` `{` in Today.** Not bound: Today's selectable lines are its cards and a card's subtasks sit inside it, not as lines of their own, so a group jump would repeat `j` and `k`. Written in `Keymap.yaml` beside the binding. `za` does fold the selected card's subtasks.
- **Mail full length on IMAP.** `full_text_bytes` is the text part's size from the body structure, before transfer decoding; on Graph it is the decoded body. It says how much was cut, not an exact character count.
- **The 120 s fetch deadline** is checked before each folder and each attempt; a call already on the wire is bounded by the transport's own timeouts (15 s connect, 60 s per read and write), so a trickling server can still overrun by up to one read timeout.

## Remaining non-blocking findings not taken

### PR 8, Home (#14)

1. A tick parses the vault four times on the main actor (tick, `today()`, `dayList`, `vaultHealth`).
2. One reorder pins every listed task to the day; a task rescheduled in Obsidian stays while the stored order names it. Needs a plan-owner decision.
3. A failed `reloadPlan` keeps the previous items, so a reorder after a failed day change can store old ids under the new date.
4. The typing guard looks at every visible window, not the key window; Adjust's date picker and segmented picker are not tracked.
5. `3d` estimates on story lines show as "no estimate" (`quick_add::parse_estimate` has no day unit).
6. A scheduled subtask whose parent has no id shows as its own card; one whose parent id has a space never shows.
7. Some app-state edits wait for an unrelated flush.
8. Two open sessions with exactly equal `started_at` are both newest, so neither is settled.
9. No midnight rollover: Home left open past midnight keeps yesterday as today until `t`.
10. Drop asymmetry: a card dropped on a card below lands after it, on a card above before it; `PlanOrder.destination(forDropBefore:from:)` is unused.

### PR 9a, calendar, Reminders and Hours (#15)

1. Two Macs running the intake at the same moment can each file the same reminder before the vault or the deletion has synced (one Mac's retry is idempotent).
2. A meeting renamed after it ended is recorded again (its id hashes the title).
3. A box that runs past midnight shows only on its first day, and only that day's pins are checked against it.
4. If storing an unpin fails after its event was deleted, the box stays locked without an event until pinned again.
5. Real EventKit (iCloud private calendar, Exchange events, Reminders deletion) is unverified without a signed build and the user's grants.

### PR 9b, Browse, Peek and previews (#16)

1. Calendar blocks are not peekable yet; `PeekSubject.block` exists but no calendar view attaches it.
2. The Browse presets still differ from the board (the core ships the six presets; Chasers is Planned).
3. Each Browse refresh reads the vault twice (the query, then `status:open` for the summary).
4. The preview module doc should say that fenced code keeps `%% ai:` lines verbatim.
5. The Peek popover's `.keyboardShortcut(.return)` is the one key outside `Keymap.yaml`.
6. Saved presets are device-local, not synced.

### PR 10, keyboard and palette (#17)

1. `gx` opens the `obsidian://` link only; a ticket URL does not reach the core's preview views.
2. `⌘⇧S` screen-share stays unbound until the mode lands.
3. Change keys (`cc` and the rest) are not repeatable with `.`; vim repeats them with the same text.
4. Recent stories live in memory only and reset on relaunch.
5. Standard `⌃1` to `⌃3` overlap Mission Control's "Switch to Desktop N" when the user turned those on.

### PR 11, timer surfaces and API (#18)

1. The CLI's plain 401 branch is reached only when the proof passes but the bearer fails (token rotated mid-call); its test runs through the failed-proof path.

### PR 12, mail (#19)

1. IMAP messages without a Message-ID fall back to `<uidvalidity>:<uid>`, so a copy in a second folder or after a UIDVALIDITY change becomes a second signal; note it in SECURITY.md or derive the fallback from the content.
2. A server that ignores `odata.maxpagesize` can return a page over the limit; every message on it is kept, so nothing is lost.
3. The Inbox placeholder text could follow `MailFetchScheduler.newSignalCount` now that cards exist.

### PR 15, story screen (#20)

1. `resolve_note` builds a path from targets containing `/` without folding `..`; safe (an index lookup) but inconsistent with `resolve_file`.
2. The keymap's prefix rule uses `overlaps`, so a story-panel `]` would refuse an unscoped `] x` chord; deliberate, kept.

### PR 13, Inbox (#21)

1. The switcher badge counts new signals only on the open workspace's row; other rows read 0 until opened.
2. Attached signals are not listed on the story screen, and the only story drop target is the story chip on a Today card.
