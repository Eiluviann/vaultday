#!/usr/bin/env bash
# Builds the Vaultday core as `VaultdayCoreFFI.xcframework` for macOS (arm64 +
# x86_64) and generates the Swift bindings into the `VaultdayCore` package.
#
# Outputs (all gitignored):
#   apps/apple/VaultdayCore/VaultdayCoreFFI.xcframework
#   apps/apple/VaultdayCore/Sources/VaultdayCore/Generated/VaultdayCore.swift
#
# The bindings generator is `crates/uniffi-bindgen`, built from the same
# pinned `uniffi` version `crates/ffi` links against. Cargo builds into
# `CARGO_TARGET_DIR` when set (`scripts/env.sh` sets it locally), else `core/target`.
#
# Usage: scripts/build-xcframework.sh [--debug]
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=scripts/env.sh
source "$repo_root/scripts/env.sh"

core_dir="$repo_root/core"
package_dir="$repo_root/apps/apple/VaultdayCore"
generated_dir="$package_dir/Sources/VaultdayCore/Generated"
xcframework="$package_dir/VaultdayCoreFFI.xcframework"
module_name="VaultdayCoreFFI"
library="libvaultday_ffi.a"
targets=(aarch64-apple-darwin x86_64-apple-darwin)
target_dir="${CARGO_TARGET_DIR:-$core_dir/target}"

profile=release
cargo_profile_flag=(--release)
if [[ "${1:-}" == "--debug" ]]; then
	profile=debug
	cargo_profile_flag=()
fi

# C dependencies (the bundled SQLite) build for the app's deployment target,
# not the host SDK's, so the app links without version warnings.
export MACOSX_DEPLOYMENT_TARGET=15.0

# The target directory may be shared with other worktrees (scripts/env.sh), and
# Cargo copies the finished library to the same path for every one of them:
# hold a lock from the build until the library is copied out.
lock_dir="$target_dir/xcframework.lock"
mkdir -p "$target_dir"
until mkdir "$lock_dir" 2>/dev/null; do
	lock_owner="$(cat "$lock_dir/pid" 2>/dev/null || true)"
	if [[ -n "$lock_owner" ]] && ! kill -0 "$lock_owner" 2>/dev/null; then
		rm -rf "$lock_dir"
		continue
	fi
	echo "==> waiting for another worktree's XCFramework build (pid ${lock_owner:-unknown})"
	sleep 2
done
echo $$ >"$lock_dir/pid"

build_dir="$(mktemp -d "${TMPDIR:-/tmp}/vaultday-xcframework.XXXXXX")"
trap 'rm -rf "$lock_dir" "$build_dir"' EXIT
rm -rf "$xcframework" "$generated_dir"
mkdir -p "$build_dir/headers" "$generated_dir"

for target in "${targets[@]}"; do
	rustup target add "$target" >/dev/null 2>&1 || true
	echo "==> cargo build -p vaultday-ffi --target $target ($profile)"
	# `${a[@]+...}`: an empty array is "unbound" under `set -u` in macOS bash 3.2.
	(cd "$core_dir" && cargo build -p vaultday-ffi --lib --target "$target" ${cargo_profile_flag[@]+"${cargo_profile_flag[@]}"})
done

universal="$build_dir/$library"
echo "==> lipo -create -> $universal"
lipo -create \
	"$target_dir/aarch64-apple-darwin/$profile/$library" \
	"$target_dir/x86_64-apple-darwin/$profile/$library" \
	-output "$universal"

echo "==> uniffi-bindgen-swift (from crates/uniffi-bindgen)"
(cd "$core_dir" && cargo run --quiet -p vaultday-uniffi-bindgen --bin uniffi-bindgen-swift -- \
	--swift-sources --module-name "$module_name" "$universal" "$generated_dir")
(cd "$core_dir" && cargo run --quiet -p vaultday-uniffi-bindgen --bin uniffi-bindgen-swift -- \
	--headers --modulemap --module-name "$module_name" --modulemap-filename module.modulemap \
	"$universal" "$build_dir/headers")

echo "==> xcodebuild -create-xcframework -> $xcframework"
xcodebuild -create-xcframework \
	-library "$universal" \
	-headers "$build_dir/headers" \
	-output "$xcframework" >/dev/null

echo "Built $xcframework"
echo "Generated $(ls "$generated_dir")"
