# Vaultday

One app for the working day: tasks, time boxes, timer, rituals and the day log over plain markdown vaults. Rust core, SwiftUI on Apple, an Electron app for Linux and Windows later, a CLI and MCP server for Claude Code.

## Layout

- `core/` Cargo workspace: `vault-base` (parser, writer, workspace resolution, settings schema), `domain` (tasks, day plan, sessions, estimates), `app-state` (Automerge document and change files, skeleton), `ffi` (UniFFI exports, with `uniffi-bindgen` for the pinned generator) and `node` (napi-rs binding, npm `@vaultday/core-node`); `core/fixtures/` holds the synthetic mock vaults the tests run against, nothing in them comes from a real vault. Planned: `features/*`.
- `tools/` `vault-migrate` (note system v1 migration), `provenance-core` (vendored AI-provenance engine), `names`, `figma`. Planned: `tokens`.
- `design/` icon sources.
- `apps/apple/` the Mac app as an XcodeGen project (`project.yml`) plus the `VaultdayCore` Swift package, which wraps the XCFramework built by `scripts/build-xcframework.sh`.
- `cli/` the `vaultday` CLI and the stdio MCP server (TypeScript on the napi binding); see [cli/README.md](cli/README.md).

## Commands

```bash
pnpm install && pnpm typecheck && pnpm lint && pnpm test
```

```bash
cd core && cargo fmt --all --check && cargo clippy --all-targets --all-features -- -D warnings && cargo test --all-features
```

The CLI's types and tests need the napi binding first: `pnpm --filter @vaultday/core-node build`. The Mac app needs the XCFramework and the generated Swift, then XcodeGen:

```bash
scripts/build-xcframework.sh && (cd apps/apple && xcodegen generate && xcodebuild build test -project Vaultday.xcodeproj -scheme Vaultday -destination 'platform=macOS' -derivedDataPath build)
```

The implementation plan (19 reviewable PRs, PR 0 to PR 18) lives in the Personal vault under `Projects/Vaultday`.
