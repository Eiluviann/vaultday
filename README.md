# Vaultday

One app for the working day: tasks, time boxes, timer, rituals and the day log over plain markdown vaults. Rust core, SwiftUI on Apple, an Electron app for Linux and Windows later, a CLI and MCP server for Claude Code.

## Layout

- `core/` Cargo workspace: `vault-base` (parser, writer, workspace resolution), `domain` (tasks, day plan, sessions, estimates). Planned: `app-state`, `features/*`, `ffi` (UniFFI) and `node` (napi-rs).
- `tools/` `vault-migrate` (note system v1 migration), `provenance-core` (vendored AI-provenance engine), `names`, `figma`. Planned: `tokens`.
- `design/` icon sources.
- Planned: `apps/apple/` (the Mac app as an XcodeGen project, plus the `VaultdayCore` Swift package) and `cli/` (the `vaultday` CLI and stdio MCP server, TypeScript on the napi binding).

## Commands

```bash
pnpm install && pnpm typecheck && pnpm lint && pnpm test
```

```bash
cd core && cargo fmt --all --check && cargo clippy --all-targets --all-features -- -D warnings && cargo test --all-features
```

The implementation plan (19 reviewable PRs, PR 0 to PR 18) lives in the Personal vault under `Projects/Vaultday`.
