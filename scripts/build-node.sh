#!/usr/bin/env bash
# Builds the Node packages and packs them into tarballs for the host platform.
#
# Outputs (gitignored):
#   dist-node/vaultday-core-node-<version>.tgz   the napi binding with its .node binary
#   dist-node/vaultday-cli-<version>.tgz         the CLI and MCP server (bundled JavaScript)
#
# Install both together so the CLI's dependency on the binding is met by the tarball:
#   pnpm add -g ./dist-node/vaultday-core-node-*.tgz ./dist-node/vaultday-cli-*.tgz
#
# Usage: scripts/build-node.sh
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
output_dir="$repo_root/dist-node"

rm -rf "$output_dir"
mkdir -p "$output_dir"

echo "==> build @vaultday/core-node (release, host platform)"
pnpm --dir "$repo_root" --filter @vaultday/core-node build

echo "==> build @vaultday/cli"
pnpm --dir "$repo_root" --filter @vaultday/cli build

echo "==> pack"
pnpm --dir "$repo_root/core/crates/node" pack --pack-destination "$output_dir"
pnpm --dir "$repo_root/cli" pack --pack-destination "$output_dir"

ls -l "$output_dir"
