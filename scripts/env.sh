# Local build environment for Vaultday. Source it, do not run it:
#
#   source scripts/env.sh
#
# Every worktree builds into its own `core/target`. Do not point
# `CARGO_TARGET_DIR` at a directory several worktrees share: Cargo names a
# workspace crate's build products after the crate's path relative to the
# workspace root, so every worktree writes the same files, and Cargo's
# freshness check compares modification times, so a worktree whose sources
# are older than another worktree's last build runs that other build's tests
# and links its library (seen here: one worktree's `cargo test` ran a deleted
# worktree's close-out tests). This file unsets an inherited target directory
# for that reason.
#
# CI does not need this (each job checks out once and caches `core/target`),
# so the file does nothing when `CI` is set. `scripts/gates.sh` and
# `scripts/build-xcframework.sh` source it themselves.

if [[ -z "${CI:-}" ]]; then
	unset CARGO_TARGET_DIR CARGO_BUILD_TARGET_DIR
	# Incremental compilation state is large and is rewritten on every build;
	# the workspace crates are small enough to rebuild whole.
	export CARGO_INCREMENTAL=0
fi
