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
| Panes and tabs | `pane.sidebar` | Sidebar | `g 0` | `⌃0` |
| Panes and tabs | `pane.today` | Today | `g 1` | `⌃1` |
| Panes and tabs | `pane.inbox` | Inbox | `g 2` | `⌃2` |
| Panes and tabs | `pane.side-panel` | Side panel | `g 3` | `⌃3` |
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
| Reports | `report.open-claude-code` | Write the report and open it in Claude Code | `y c` · `y c` | `⌥⌘C` · `⌥⌘C` |
| Panes and tabs | `sidebar.toggle` | Show or hide the sidebar | `⌃⌘S` | `⌃⌘S` |
| Day and workspace | `plan.auto-schedule` | Arrange the day in list order | `=` | `⇧⌘R` |
| Day and workspace | `plan.replan` | Re-plan from now | `r` | `⌘R` |
| Day and workspace | `day.previous` | Previous day | `⌥H` | `⌘[` |
| Day and workspace | `day.next` | Next day | `⌥L` | `⌘]` |
| Day and workspace | `day.today` | Back to today | `g d` | `⌘T` |
| Day and workspace | `daylog.open` | Day log | `g l` | `⇧⌘L` |
| Day and workspace | `daylog.append` | Append to the day log | `⌘L` | `⌘L` |
| Day and workspace | `workspace.next` | Next workspace | `g t` | `⌃⌘]` |
| Day and workspace | `workspace.previous` | Previous workspace | `g T` | `⌃⌘[` |
| Day and workspace | `workspace.all` | All workspaces | `⌘0` | `⌘0` |
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
