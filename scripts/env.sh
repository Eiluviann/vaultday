# Local build environment for Vaultday. Source it, do not run it:
#
#   source scripts/env.sh
#
# One Cargo target directory serves every worktree, so the third-party crates
# (uniffi, automerge, rusqlite with its bundled SQLite, tokio) compile once per
# Mac instead of once per worktree. Workspace crates still build per worktree:
# their path is part of the build hash, so worktrees on different branches do
# not overwrite each other's build products.
#
# CI does not need this: it keeps `core/target` and caches it per job, so the
# file does nothing when `CI` is set. `scripts/gates.sh` and
# `scripts/build-xcframework.sh` source it themselves.

if [[ -z "${CI:-}" ]]; then
	export CARGO_TARGET_DIR="${VAULTDAY_CARGO_TARGET:-$HOME/Library/Caches/vaultday/cargo-target}"
	# Incremental compilation state is large and is rewritten on every build;
	# the workspace crates are small enough to rebuild whole.
	export CARGO_INCREMENTAL=0
	mkdir -p "$CARGO_TARGET_DIR"
fi
