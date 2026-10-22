---
Number: STRY0000001
Ticket: "[Open in ServiceNow](https://example.service-now.com/nav_to.do?uri=rm_story.do%3Fsysparm_query=number=STRY0000001)"
Status: In progress
sort_index: 1000
note-system-ver: 0.1
---
# Description
The widget gets a colour picker. See [[Current widget]] and the export in `exploration/export.txt`.
![[STRY0000001.jpg]]
# Technical Solution Details
## Background
Example lines that must stay untouched:
```markdown
- [ ] a task inside a fence [assigned_to::GBS]
[[Widget colour picker (new look)]]
%% ai:begin claude-x · 2026-01-01 · 000000000000 %%
```
## Implementation
%% ai:begin claude-test-1 · 2026-10-01 · e4fbd3c56862 %%
Apply the colour map on load.
- [ ] Verify on dev: the picker honours the theme  [created:: 2026-10-12] [assigned_to::me]
%% ai:end %%
## Pending Input
- [X] Alignment from business on the palette  [created:: 2026-07-09] [assigned_to::GBS] [completion:: 2026-10-01]
- [ ] Colour codes for the dark theme  [created:: 2026-07-09] [assigned_to::GBS]
	- Nested note under the task, not a task
## Notes
- [ ] Loose checkbox without fields
# Acceptance Criteria
1. Picker visible.
