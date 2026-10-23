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
| Normal | `h` `l` `w` `b` `e` `0` `^` `$` move (`j` `k` `gg` `G` on several lines); counts (`3w`, `2dw`); `i` `a` `I` `A` insert (`o` `O` open a line on several lines); `x` `X` `dd` `dw` `de` `db` `d$` `D` delete; `cw` `cc` `C` `s` `S` change; `yy` `yw` yank; `p` `P` put; `u` undo; ⌃R redo; `.` repeats the last change; `v` Visual; ⏎ submits a one-line field; Esc leaves the field |
| Insert | typing as usual, a bar cursor; Esc returns to Normal mode; ⏎ submits a one-line field |
| Visual | the motions extend the selection; `d` `x` delete, `c` change, `y` yank, `o` swaps the ends; Esc or `v` back to Normal mode |

⌘ keys always reach the app from inside a field. Password fields stay plain fields under both schemes.

## The key left of 1

⌘ plus the key left of 1 shows all workspaces. It is matched by its place, not its character: § on an ISO keyboard, ` on an ANSI one. On an ANSI keyboard macOS's own "Move focus to next window" takes ⌘` first unless it is turned off in System Settings › Keyboard › Keyboard Shortcuts.
