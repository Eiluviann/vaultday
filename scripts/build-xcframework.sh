#!/usr/bin/env bash
# Builds the Vaultday core as `VaultdayCoreFFI.xcframework` for macOS (arm64 +
# x86_64) and generates the Swift bindings into the `VaultdayCore` package.
#
# Outputs (all gitignored):
#   apps/apple/VaultdayCore/VaultdayCoreFFI.xcframework
#   apps/apple/VaultdayCore/Sources/VaultdayCore/Generated/VaultdayCore.swift
#
# The bindings generator is `crates/uniffi-bindgen`, built from the same
# pinned `uniffi` version `crates/ffi` links against.
#
# Usage: scripts/build-xcframework.sh [--debug]
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
core_dir="$repo_root/core"
package_dir="$repo_root/apps/apple/VaultdayCore"
generated_dir="$package_dir/Sources/VaultdayCore/Generated"
xcframework="$package_dir/VaultdayCoreFFI.xcframework"
module_name="VaultdayCoreFFI"
library="libvaultday_ffi.a"
targets=(aarch64-apple-darwin x86_64-apple-darwin)

profile=release
cargo_profile_flag=(--release)
if [[ "${1:-}" == "--debug" ]]; then
	profile=debug
	cargo_profile_flag=()
fi

build_dir="$core_dir/target/xcframework"
rm -rf "$build_dir" "$xcframework" "$generated_dir"
mkdir -p "$build_dir/headers" "$generated_dir"

for target in "${targets[@]}"; do
	rustup target add "$target" >/dev/null 2>&1 || true
	echo "==> cargo build -p vaultday-ffi --target $target ($profile)"
	(cd "$core_dir" && cargo build -p vaultday-ffi --lib --target "$target" "${cargo_profile_flag[@]}")
done

universal="$build_dir/$library"
echo "==> lipo -create -> $universal"
lipo -create \
	"$core_dir/target/aarch64-apple-darwin/$profile/$library" \
	"$core_dir/target/x86_64-apple-darwin/$profile/$library" \
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
