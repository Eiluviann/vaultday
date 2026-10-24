#!/usr/bin/env bash
# Generates or resets the synthetic playground vaults (default ~/Vaultday-playground)
# and the command that opens Vaultday on them. Arguments go to tools/playground:
# --out <folder>, --today YYYY-MM-DD, --now HH:MM.
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
# tsx runs from the caller's folder, so a relative --out means what it says.
exec "$repo_root/tools/playground/node_modules/.bin/tsx" "$repo_root/tools/playground/src/main.ts" "$@"
