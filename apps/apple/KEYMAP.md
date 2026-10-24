# Vaultday keys

Generated from `apps/apple/Keymap.yaml` by `tools/keymap-doc`; do not edit by hand. Run `pnpm --filter @vaultday/keymap-doc generate` after changing the keymap.

| Group | Action | Label | Vim motions | Standard |
| --- | --- | --- | --- | --- |
| Selected task | `open` | Open the task's preview | `⏎` | `⏎` |
| Selected task | `peek` | Peek | `K` | `⌘Y` |
| Selected task | `timer.toggle` | Start the timer, or pause and resume it | `Space` | `Space` |
| Selected task | `task.done` | Done, writes [x] in the note | `x` | `⌘D` |
| Selected task | `task.pin` | Lock the calendar slot, or unlock it | `m` | `⌥⌘L` |
| Selected task | `task.push-tomorrow` | Push to tomorrow | `> >` | `⌃⌘→` |
| Selected task | `task.pull-today` | Pull to today | `< <` | `⌃⌘←` |
| Selected task | `task.move-up` | Move the task up | `⌥K` | `⌥⌘↑` |
| Selected task | `task.move-down` | Move the task down | `⌥J` | `⌥⌘↓` |
| Selected task | `item.menu` | More actions | `\` | `⌃⏎` |
| Selected task | `item.open-link` | Open the link | `g x` | `⇧⌘O` |
| Selected task | `item.claude-code` | Open in Claude Code | `⇧⌘C` | `⇧⌘C` |
| Selected signal | `signal.task` | + Task, a line under Next Steps | `1` | `1` |
| Selected signal | `signal.wait-for` | Wait for the sender, a line under Pending Input | `2` | `2` |
| Selected signal | `signal.attach` | Attach to a story | `3` | `3` |
| Selected signal | `signal.dismiss` | Dismiss, writes nothing | `4` | `4` |
| Selected signal | `signal.snooze` | Snooze until a date | `> >` | `⌃⌘→` |
| Selected signal | `signal.reply` | Reply in the mail app | — | — |
| Selected signal | `signal.not-a-signal` | Not a signal | — | — |
| Change | `change.text` | Change the text | `c c` | `⌘E` |
| Change | `change.estimate` | Change the estimate | `c e` | `⌥⌘E` |
| Change | `change.due` | Change the due day | `c d` | `⇧⌘D` |
| Change | `change.scheduled` | Change the scheduled day | `c s` | `⌥⌘S` |
| Register | `register.yank` | Yank the task and copy its id | `y y` | `⌘C` |
| Register | `register.cut` | Cut to the backlog | `d d` | `⌘X` |
| Register | `register.paste-after` | Paste after the selection, scheduled on this day | `p` | `⌘V` |
| Register | `register.paste-before` | Paste before the selection, scheduled on this day | `P` | `⇧⌘V` |
| Register | `edit.undo` | Undo | `u` | `⌘Z` |
| Register | `edit.redo` | Redo | `⌃R` | `⇧⌘Z` |
| Register | `edit.repeat` | Repeat the last change | `.` | — |
| Navigation | `move.down` | Move down | `j` | `↓` |
| Navigation | `move.up` | Move up | `k` | `↑` |
| Navigation | `move.left` | Move left | `h` | `←` |
| Navigation | `move.right` | Move right | `l` | `→` |
| Navigation | `move.first` | First item | `g g` | `⌘↑` |
| Navigation | `move.last` | Last item | `G` | `⌘↓` |
| Navigation | `move.half-page-down` | Half a page down | `⌃D` | `⇟` |
| Navigation | `move.half-page-up` | Half a page up | `⌃U` | `⇞` |
| Navigation | `task.add` | Quick add | `i` | `⌘N` |
| Navigation | `task.add-after` | Add a task after the selection | `o` | `⌥⌘N` |
| Navigation | `task.add-before` | Add a task before the selection | `O` | `⌥⇧⌘N` |
| Navigation | `palette.open` | Command palette | `⌘K` · `:` | `⌘K` |
| Navigation | `dismiss` | Close the palette, the toast or the confirmation | `Esc` | `Esc` |
| Navigation | `settings.toggle` | Open or close Settings | `⌘,` | `⌘,` |
| Panes and tabs | `pane.sidebar` | Sidebar | `g 1` | `⌃1` |
| Panes and tabs | `pane.today` | Today | `g 2` | `⌃2` |
| Panes and tabs | `pane.inbox` | Inbox | `g 3` | `⌃3` |
| Panes and tabs | `pane.side-panel` | Side panel | `g 4` | `⌃4` |
| Panes and tabs | `side-panel.next` | Next side panel tab | `]` | `⌃⇥` |
| Panes and tabs | `side-panel.previous` | Previous side panel tab | `[` | `⌃⇧⇥` |
| Panes and tabs | `side-panel.tab.1` | Calendar tab | `⌥1` · `1` | `⌥1` |
| Panes and tabs | `side-panel.tab.2` | Hours tab | `⌥2` · `2` | `⌥2` |
| Panes and tabs | `side-panel.tab.3` | Chasers tab | `⌥3` · `3` | `⌥3` |
| Panes and tabs | `side-panel.tab.4` | Backlog tab | `⌥4` · `4` | `⌥4` |
| Panes and tabs | `side-panel.tab.5` | Objectives tab | `⌥5` · `5` | `⌥5` |
| Panes and tabs | `side-panel.tab.6` | Day log tab | `⌥6` · `6` | `⌥6` |
| Panes and tabs | `side-panel.tab.7` | Claude tab | `⌥7` · `7` | `⌥7` |
| Panes and tabs | `side-panel.tab.8` | Reports tab | `⌥8` · `8` | `⌥8` |
| Panes and tabs | `story-panel.next` | Next story panel tab | `]` | `⌃⇥` |
| Panes and tabs | `story-panel.previous` | Previous story panel tab | `[` | `⌃⇧⇥` |
| Panes and tabs | `story-panel.tab.1` | Story Hours tab | `1` | `⌥1` |
| Panes and tabs | `story-panel.tab.2` | Story Reports tab | `2` | `⌥2` |
| Panes and tabs | `story-panel.tab.3` | Story Claude Code tab | `3` | `⌥3` |
| Reports | `report.copy-json` | Copy the report as JSON | `y j` · `y j` | `⇧⌘J` · `⇧⌘J` |
| Reports | `report.copy-markdown` | Copy the report as markdown | `y m` · `y m` | `⇧⌘M` · `⇧⌘M` |
| Reports | `report.open-claude-code` | Write the report for Claude Code | `y c` · `y c` | `⌥⌘C` · `⌥⌘C` |
| Panes and tabs | `sidebar.toggle` | Show or hide the sidebar | `⌃⌘S` | `⌃⌘S` |
| Panes and tabs | `screen-share.toggle` | Turn screen-share mode on or off | `⇧⌘S` | `⇧⌘S` |
| Day and workspace | `plan.auto-schedule` | Arrange the day in list order | `=` | `⇧⌘R` |
| Day and workspace | `plan.replan` | Re-plan from now | `r` | `⌘R` |
| Day and workspace | `day.previous` | Previous day | `⌥H` | `⌘[` |
| Day and workspace | `day.next` | Next day | `⌥L` | `⌘]` |
| Day and workspace | `day.today` | Back to today | `g d` | `⌘T` |
| Day and workspace | `view.list` | Show the day as a list | — | — |
| Day and workspace | `view.three-days` | Show three days | — | — |
| Day and workspace | `view.week` | Show the week | — | — |
| Day and workspace | `daylog.open` | Day log | `g l` | `⇧⌘L` |
| Day and workspace | `ritual.close-out` | Week close-out | `g W` | `⇧⌘K` |
| Day and workspace | `daylog.append` | Append to the day log | `⌘L` | `⌘L` |
| Day and workspace | `workspace.next` | Next workspace | `g t` | `⌃⌘]` |
| Day and workspace | `workspace.previous` | Previous workspace | `g T` | `⌃⌘[` |
| Day and workspace | `workspace.all` | All workspaces | `⌘§` | `⌘§` |
| Day and workspace | `workspace.1` | Workspace 1 | `⌘1` | `⌘1` |
| Day and workspace | `workspace.2` | Workspace 2 | `⌘2` | `⌘2` |
| Day and workspace | `workspace.3` | Workspace 3 | `⌘3` | `⌘3` |
| Day and workspace | `workspace.4` | Workspace 4 | `⌘4` | `⌘4` |
| Day and workspace | `workspace.5` | Workspace 5 | `⌘5` | `⌘5` |
| Day and workspace | `workspace.6` | Workspace 6 | `⌘6` | `⌘6` |
| Day and workspace | `workspace.7` | Workspace 7 | `⌘7` | `⌘7` |
| Day and workspace | `workspace.8` | Workspace 8 | `⌘8` | `⌘8` |
| Day and workspace | `workspace.9` | Workspace 9 | `⌘9` | `⌘9` |
| Browse | `browse.open` | Browse | `g b` | `⇧⌘B` |
| Browse | `browse.query` | Search the Browse table | `/` | `⌘F` |
| Browse | `browse.next-match` | Next match | `n` | `⌘G` |
| Browse | `browse.previous-match` | Previous match | `N` | `⇧⌘G` |
| Browse | `browse.next-group` | Next story group | `}` | `⌥↓` |
| Browse | `browse.previous-group` | Previous story group | `{` | `⌥↑` |
| Browse | `browse.fold` | Fold or open the story group, or the card's subtasks | `z a` | — |
| Panes and tabs | `tracker.toggle` | Show or hide the mini tracker | `⌥⌘T` | `⌥⌘T` |
| Menu bar | `menu-bar.previous` | Previous row of the timer menu | `↑` | `↑` |
| Menu bar | `menu-bar.next` | Next row of the timer menu | `↓` | `↓` |
| Menu bar | `menu-bar.start` | Start the timer on the row or the typed entry | `⏎` | `⏎` |
| Menu bar | `menu-bar.new-entry` | Type a new timer entry | `⌘N` | `⌘N` |
| Menu bar | `menu-bar.cancel` | Leave the typed entry | `Esc` | `Esc` |

## Focus and Esc

The keys go to the deepest level that holds focus: the window, a pane, a section inside a pane, a row, or a text input. Esc moves one level up, after any palette, popover, preview or toast has closed.

| Level | Reached with | Esc goes to |
| --- | --- | --- |
| Pane | `g1` sidebar, `g2` Today, `g3` Inbox, `g4` side panel (⌃1 to ⌃4); `h` `l` (← →) step across | stays: a pane is as high as Esc goes |
| Section | the Today header: `k` (↑) above the first card | the pane (Vim motions: the pane itself; Standard: its first card) |
| Row | `j` `k` (↓ ↑) | Vim motions: the pane itself; Standard: stays |
| Input | `i` quick add, `o` `O` add after or before (⌘N, ⌥⌘N, ⇧⌥⌘N), `/` the Browse query, a click | the pane the input belongs to, and `hjkl` act on the next key |

- The sidebar's rows are the workspaces, the Now card, the rituals, Browse, the health dot and the Settings gear. `g1` lands on the current row (the open workspace, or the row last highlighted); ⏎ does what a click does.
- The Today header is a section of the Today pane: `k` above the first card enters it, `h` `l` walk the today chip, ‹ and ›, the List / 3 days / Week switch, `=` and the wand, ⏎ presses the highlighted one (on the span switch ⏎ steps to the next span), `j` comes back to the first card. Each control also has its own keys: `gd` today, ⌥h ⌥l the previous and next day, `=` arrange the day, `r` re-plan; the spans are in the palette.
- `[` `]` step the tabs of the focused pane. A pane without tabs of its own falls back to the only tabbed pane on screen: the side panel on Home, the story panel on the story screen.
- A pending prefix (`g`, `c`, `y`, `d`, `z`, `>`) waits for its next key as long as it takes, as in an editor; Esc drops it and does nothing else.

## Vim inside text inputs

Under Vim motions every text input has its own Vim modes, shown under its bottom-left corner. Focusing a field puts it in Normal mode with a block cursor; `i` `o` `O` and `/` open their field in Insert mode, because that is what those keys mean. Under Standard the fields are plain macOS fields.

| Mode | Keys |
| --- | --- |
| Normal | `h` `l` `w` `b` `e` `0` `^` `$` move (`j` `k` `gg` `G` on several lines); `f` `t` `F` `T` find a character on the line, `;` `,` repeat the find; counts (`3w`, `2dw`); `i` `a` `I` `A` insert (`o` `O` open a line on several lines); `x` `X` `dd` `dw` `de` `db` `d$` `dt,` `D` delete; `cw` `cc` `C` `s` `S` change; `r` replaces the character under the cursor; `yy` `yw` yank; `p` `P` put; `u` undo; ⌃R redo; `.` repeats the last change; `v` Visual; ⏎ submits a one-line field; Esc or ⌃[ leaves the field. Marks, macros and named registers are not kept, but `m` `q` `@` `'` `"` still take their character |
| Insert | typing as usual, a bar cursor; Esc returns to Normal mode; ⏎ submits a one-line field |
| Visual | the motions extend the selection; `d` `x` delete, `c` change, `y` yank, `r` replaces every selected character, `o` swaps the ends; Esc or `v` back to Normal mode |

⌘ keys always reach the app from inside a field. Password fields stay plain fields under both schemes.

## The key left of 1

⌘ plus the key left of 1 shows all workspaces. It is matched by its place, not its character: § on an ISO keyboard, ` on an ANSI one. On an ANSI keyboard macOS's own "Move focus to next window" takes ⌘` first unless it is turned off in System Settings › Keyboard › Keyboard Shortcuts.
