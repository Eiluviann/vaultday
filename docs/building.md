# Building and gating locally

How agents (and people) build Vaultday and run the gates on this Mac, several git worktrees at a time, without paying for a cold build on every run.

## The workflow

```bash
source scripts/env.sh          # once per shell: the shared Cargo target directory
scripts/gates.sh --changed     # while iterating: only the gates your changes touch
scripts/gates.sh --full        # once, before opening or updating the PR
```

- **Implementers** run `--changed` after each step and `--full` once before pushing.
- **Reviewers** run `--changed` against the PR's branch and let CI carry the full matrix (Linux and macOS, the release XCFramework with both architectures). The repository is public, so CI minutes cost nothing.
- Most of the Rust test gate is the test binaries running one after another. `cargo nextest` runs them in parallel, and `gates.sh` uses it when it is installed (`cargo install cargo-nextest --locked`).
- Both print a time per gate and a summary table. A failing gate prints the last 40 lines of its log; the full logs are in `$TMPDIR/vaultday-gates/<worktree>/`. `--verbose` streams everything instead.
- `--changed` compares against `origin/main` (`git fetch` first); `--base <ref>` picks another base.

## What each mode runs

`--full` is what CI runs:

| Area | Gates |
| --- | --- |
| Rust | `cargo fmt --check`, `cargo clippy --all-targets --all-features -D warnings`, `cargo test --all-features` (`cargo nextest run` plus the doc tests when nextest is installed) |
| Apple | `scripts/build-xcframework.sh`, `xcodegen generate`, one `xcodebuild test` of the `Vaultday` scheme, which holds the app tests and the `VaultdayCore` package tests |
| Node | `pnpm install`, the napi binding (debug), `pnpm -r typecheck`, `pnpm -r test`, biome, the tokens and keymap checks |

`--changed` runs a gate only when a file it reads changed since the base:

| Changed | Gates |
| --- | --- |
| `core/crates/<crate>/` | fmt, then clippy and tests with `-p` for that crate and every workspace crate depending on it |
| `core/Cargo.toml`, `Cargo.lock`, `core/fixtures/`, `rust-toolchain.toml` | every Rust gate for the whole workspace |
| `apps/apple/`, `core/crates/ffi/`, `core/crates/uniffi-bindgen/`, `scripts/build-xcframework.sh` | the XCFramework, xcodegen, `xcodebuild test` |
| `cli/`, `tools/<package>/`, `core/crates/node/` | the napi binding, then typecheck and vitest for those packages and their dependents |
| a crate `crates/node` depends on (`vault-base`, `domain`) | also the napi binding and the CLI's typecheck and vitest |
| root `package.json`, `pnpm-lock.yaml`, `tsconfig.base.json` | every Node gate |
| a `.ts`, `.js`, `.json` or `.css` file | biome |
| `apps/apple/Keymap.yaml` | `pnpm --filter @vaultday/keymap-doc check` |
| `design/tokens.json`, `tools/tokens/`, `Tokens.swift` | `pnpm --filter @vaultday/tokens check` |

A change to a crate the FFI links (`domain`, say) runs its Rust tests here; the Swift side of it is left to `--full` and CI. `swift test` in `apps/apple/VaultdayCore` still works for a quick run of the package alone, but no gate needs it: the scheme runs the package tests.

## Caches: keep them

Keep `core/target` (or the shared directory below), `apps/apple/build` and `apps/apple/VaultdayCore/.build` between runs: measured on this Mac, a warm `--full` takes about 40 seconds, a fresh worktree about 2 minutes (the shared Cargo cache is warm), a cold Mac about 3.5 minutes, and `--changed` 20 to 40 seconds for a one-line change. Delete them only when the disk has less than 10 GB free (`df -h ~`), and then the shared Cargo directory first, since it is the largest.

- **Shared Cargo target.** `scripts/env.sh` points `CARGO_TARGET_DIR` at `~/Library/Caches/vaultday/cargo-target` (or `$VAULTDAY_CARGO_TARGET` when set), one directory for every worktree. Third-party crates compile once per Mac. Workspace crates build per worktree, since their path is part of the build hash, so branches do not overwrite each other. Two worktrees building at the same moment take turns on Cargo's lock ("Blocking waiting for file lock"); that is expected. The file does nothing on CI, which caches `core/target` per job. `gates.sh` and `build-xcframework.sh` source it themselves; source it in your shell too, so a bare `cargo test` uses the same cache.
- **Debug info.** The committed dev profile (`core/Cargo.toml`) keeps line tables for the workspace crates and no debug info for dependencies, so nobody has to remember `CARGO_PROFILE_DEV_DEBUG=0`. `scripts/env.sh` turns incremental compilation off, which is what `CARGO_INCREMENTAL=0` did.
- **XCFramework.** Locally `scripts/build-xcframework.sh` builds an unoptimised arm64-only framework, and skips the build when the core sources, the toolchain and the build kind match the last build in this worktree (`--force` rebuilds). CI and `--release` build the optimised arm64 and x86_64 framework that ships. Use `--release` for a build you run day to day: the debug core is slower.
- **DerivedData.** `xcodebuild` writes to `apps/apple/build` (gitignored), Debug only, for the Mac's own architecture, without the index store Xcode's editor uses.

## Signing

The gates build with signing off, like CI. A Personal Team build of the app expires after 7 days and needs a weekly rebuild from Xcode: [docs/signing.md](signing.md).
