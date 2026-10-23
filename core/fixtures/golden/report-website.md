# Report: Website

Client · 2026-10-21 to 2026-10-22

## Summary

- Stories: 2
- Log lines: 8
- Sessions: 2 (0m logged, 2 without a duration)
- Billing: not set

## Stories

### WEB-142 · Checkout redesign

- Status: in-progress
- Note: Projects/Website/WEB-142 Checkout redesign/WEB-142 Checkout redesign.md
- source_host: tracker.example.test
- source_kind: issue
- source_id: 10142
- priority: high
- due: 2026-10-30
- estimate: 3d
- sort_index: 10
- aliases: Checkout v2, Cart redesign
- note_system: 1.0

Next Steps (4: 3 open, 1 in progress)

- [ ] Wire the payment form to the gateway sandbox (task-2 · estimate 2h)
  - [ ] Add the gateway test keys to the `.env.example` (task-3)
  - [/] Handle the declined-card response (task-4 · estimate 45m)
- [ ] Write the end-to-end checkout test (task-5 · due 2026-10-28)

Pending Input (2: 1 open, 1 done)

- [ ] Final copy for the order summary (task-6 · with Dana)
- [x] Confirm the tax rounding rule (task-7 · with Lee · done 2026-10-19)

### WEB-157 · Newsletter signup

- Status: ready
- Note: Projects/Website/WEB-157 Newsletter signup/WEB-157 Newsletter signup.md
- source_host: tracker.example.test
- source_kind: issue
- source_id: 10157
- priority: medium
- estimate: 1d
- sort_index: 20
- note_system: 1.0

Next Steps (1: 1 open)

- [ ] Draft the confirmation mail (task-8)

Pending Input (1: 1 open)

- [ ] Mailing-list provider decision (task-9 · with Lee)

## Log

### Status (1)

- 2026-10-21 09:10 Planned the checkout work (WEB-142)

### Decision (1)

- 2026-10-21 10:00 Coupon field stays above the fold (WEB-142)

### Session (2)

- 2026-10-21 10:05 Session on the payment form (WEB-142 · task-2)
- 2026-10-22 09:30 Session on the end-to-end test (WEB-142 · task-5)

### Blocker (1)

- 2026-10-21 12:30 Gateway sandbox returns 502 on refunds (WEB-142)

### Done (2)

- 2026-10-22 08:45 Address form extracted and reviewed (WEB-142 · task-1)
- 2026-10-22 11:15 Testing steps drafted (Website · written by claude-fable-5-1)

### Note (1)

- 2026-10-22 14:00 Asked Lee again about the provider (WEB-157 · task-9)

## Hours

Logged: 0m

## Authorship

Computed by the CLI (vaultday report)
