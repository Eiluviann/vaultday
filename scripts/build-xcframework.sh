#!/usr/bin/env bash
# Builds the Vaultday core as `VaultdayCoreFFI.xcframework` for macOS and
# generates the Swift bindings into the `VaultdayCore` package.
#
# Two builds:
#   release  arm64 + x86_64, optimised. The default on CI (`CI` set) and with
#            `--release`: what ships and what the app runs day to day.
#   debug    arm64 only, unoptimised. The local default: it compiles in a
#            fraction of the time and is enough for `xcodebuild test`.
#
# A build is skipped when the core sources, the toolchain and the build kind
# match the last one in this worktree (`--force` builds anyway).
#
# Outputs (all gitignored):
#   apps/apple/VaultdayCore/VaultdayCoreFFI.xcframework
#   apps/apple/VaultdayCore/Sources/VaultdayCore/Generated/VaultdayCore.swift
#   apps/apple/VaultdayCore/.xcframework-stamp
#
# The bindings generator is `crates/uniffi-bindgen`, built from the same
# pinned `uniffi` version `crates/ffi` links against. Cargo builds into
# `CARGO_TARGET_DIR` when set (`scripts/env.sh` sets it locally), else `core/target`.
#
# Usage: scripts/build-xcframework.sh [--release | --debug] [--force]
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=scripts/env.sh
source "$repo_root/scripts/env.sh"

core_dir="$repo_root/core"
package_dir="$repo_root/apps/apple/VaultdayCore"
generated_dir="$package_dir/Sources/VaultdayCore/Generated"
xcframework="$package_dir/VaultdayCoreFFI.xcframework"
stamp_file="$package_dir/.xcframework-stamp"
module_name="VaultdayCoreFFI"
library="libvaultday_ffi.a"
target_dir="${CARGO_TARGET_DIR:-$core_dir/target}"

profile=debug
if [[ -n "${CI:-}" ]]; then
	profile=release
fi
force=false
for argument in "$@"; do
	case "$argument" in
		--release) profile=release ;;
		--debug) profile=debug ;;
		--force) force=true ;;
		*)
			echo "usage: scripts/build-xcframework.sh [--release | --debug] [--force]" >&2
			exit 64
			;;
	esac
done

if [[ "$profile" == release ]]; then
	targets=(aarch64-apple-darwin x86_64-apple-darwin)
	cargo_profile_flag=(--release)
else
	targets=(aarch64-apple-darwin)
	cargo_profile_flag=()
fi

# C dependencies (the bundled SQLite) build for the app's deployment target,
# not the host SDK's, so the app links without version warnings.
export MACOSX_DEPLOYMENT_TARGET=15.0

# Everything the XCFramework is built from, hashed by content: tracked and
# untracked files under core/ (minus the napi crate, which the FFI does not
# link), the toolchain pin and this script.
function source_fingerprint() {
	{
		echo "$profile ${targets[*]}"
		# A tracked file deleted in the working tree has nothing to hash.
		(cd "$repo_root" && git ls-files -z --cached --others --exclude-standard -- \
			core rust-toolchain.toml scripts/build-xcframework.sh scripts/env.sh ':!core/crates/node' |
			xargs -0 shasum -a 256 2>/dev/null) || true
	} | shasum -a 256 | cut -d ' ' -f 1
}

fingerprint="$(source_fingerprint)"
if [[ "$force" == false && -d "$xcframework" && -d "$generated_dir" && -f "$stamp_file" &&
	"$(cat "$stamp_file")" == "$fingerprint" ]]; then
	echo "XCFramework is up to date ($profile, ${targets[*]}); --force rebuilds it"
	exit 0
fi

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
rm -rf "$xcframework" "$generated_dir" "$stamp_file"
mkdir -p "$build_dir/headers" "$generated_dir"

slices=()
for target in "${targets[@]}"; do
	rustup target add "$target" >/dev/null 2>&1 || true
	echo "==> cargo build -p vaultday-ffi --target $target ($profile)"
	# `${a[@]+...}`: an empty array is "unbound" under `set -u` in macOS bash 3.2.
	(cd "$core_dir" && cargo build -p vaultday-ffi --lib --target "$target" ${cargo_profile_flag[@]+"${cargo_profile_flag[@]}"})
	cp "$target_dir/$target/$profile/$library" "$build_dir/$target-$library"
	slices+=("$build_dir/$target-$library")
done

combined="$build_dir/$library"
echo "==> lipo -create -> $combined"
lipo -create "${slices[@]}" -output "$combined"

echo "==> uniffi-bindgen-swift (from crates/uniffi-bindgen)"
(cd "$core_dir" && cargo run --quiet -p vaultday-uniffi-bindgen --bin uniffi-bindgen-swift -- \
	--swift-sources --module-name "$module_name" "$combined" "$generated_dir")
(cd "$core_dir" && cargo run --quiet -p vaultday-uniffi-bindgen --bin uniffi-bindgen-swift -- \
	--headers --modulemap --module-name "$module_name" --modulemap-filename module.modulemap \
	"$combined" "$build_dir/headers")

echo "==> xcodebuild -create-xcframework -> $xcframework"
xcodebuild -create-xcframework \
	-library "$combined" \
	-headers "$build_dir/headers" \
	-output "$xcframework" >/dev/null

echo "$fingerprint" >"$stamp_file"
echo "Built $xcframework ($profile, ${targets[*]})"
echo "Generated $(ls "$generated_dir")"
