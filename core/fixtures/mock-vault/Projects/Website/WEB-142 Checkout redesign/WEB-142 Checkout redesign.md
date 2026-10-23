---
type: story
number: WEB-142
source_host: tracker.example.test
source_kind: issue
source_id: "10142"
status: in-progress
priority: high
due: 2026-10-30
estimate: 3d
sort_index: 10
aliases:
  - Checkout v2
  - Cart redesign
note_system: "1.0"
---
# Checkout redesign

Replace the three-step checkout with a single page. Ticket: [WEB-142](https://tracker.example.test/browse/WEB-142).

## Background
Example lines that must stay untouched by any tool:
```markdown
- [ ] a task inside a fence [assigned_to::Dana]
%% ai:begin fable · 2026-10-01 · 000000000000 %%
```

> [!note] Scope
> Card payments only; wallets follow in a later story.

See [[Research notes]] and [[Research notes#Checkout research|the competitor sweep]].

![[checkout-sketch.svg]]

## Implementation
%% ai:begin fable · 2026-10-20 · 3f9a1c2b7e4d %%
Render the address and payment forms side by side; validate on blur.
- [x] Extract the address form into its own component  [id:: task-1] [created:: 2026-10-18] [assigned_to:: me] [completion:: 2026-10-20]
%% ai:end %%

## Next Steps
- [ ] Wire the payment form to the gateway sandbox  [id:: task-2] [created:: 2026-10-20] [scheduled:: 2026-10-22] [estimate:: 2h]
	- [ ] Add the gateway test keys to the `.env.example`  [id:: task-3] [created:: 2026-10-21]
	- [/] Handle the declined-card response  [id:: task-4] [created:: 2026-10-21] [estimate:: 45m]
- [ ] Write the end-to-end checkout test  [id:: task-5] [created:: 2026-10-21] [due:: 2026-10-28]

## Pending Input
- [ ] Final copy for the order summary  [id:: task-6] [created:: 2026-10-10] [assigned_to:: Dana]
- [x] Confirm the tax rounding rule  [id:: task-7] [created:: 2026-10-12] [assigned_to:: Lee] [completion:: 2026-10-19]
	- Rounds half up per line, confirmed by mail

## Notes
- 2026-10-21 Decided to keep the coupon field above the fold.

## Activity
```dataview
TABLE WITHOUT ID time, text FROM "Log" WHERE story = "WEB-142"
```
