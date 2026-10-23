#!/usr/bin/env bash
# The local gate: one entry point for the checks CI runs, with a time per gate.
#
#   scripts/gates.sh --full      every gate, as CI runs them (the implementer's
#                                last check before opening or updating a PR)
#   scripts/gates.sh --changed   only the gates the changes since the base
#                                touch (while iterating, and for reviewers)
#
# Options:
#   --base <ref>   what --changed compares against (default: origin/main;
#                  the merge base with HEAD is used)
#   --verbose      stream every gate's output instead of logging it
#
# What --changed runs, from the files changed since the base (committed,
# staged, unstaged and untracked):
#   Rust   cargo fmt, then clippy and tests for the crates whose files changed
#          plus every workspace crate that depends on them (`-p` per crate);
#          the whole workspace when Cargo.toml, Cargo.lock, the toolchain pin
#          or core/fixtures changed
#   Apple  the XCFramework, xcodegen and `xcodebuild test` (app, app tests and
#          the VaultdayCore package tests) when apps/apple, crates/ffi,
#          crates/uniffi-bindgen or scripts/build-xcframework.sh changed
#   Node   the napi binding, typecheck and vitest for the changed pnpm
#          packages and their dependents (all of them when the root manifests
#          changed; the binding and the CLI when a Rust change reaches
#          crates/node), biome when a file biome reads changed
#   Keymap `@vaultday/keymap-doc check` when apps/apple/Keymap.yaml changed
#   Tokens `@vaultday/tokens check` when design/tokens.json, tools/tokens or
#          Tokens.swift changed
#
# Uses `cargo nextest` when installed, `cargo test` otherwise. Logs go to
# $TMPDIR/vaultday-gates/<worktree>/; a failing gate prints its last lines.
# Exits non-zero when any gate fails. Sources scripts/env.sh (shared Cargo
# target directory).
set -uo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=scripts/env.sh
source "$repo_root/scripts/env.sh"

core_dir="$repo_root/core"
apple_dir="$repo_root/apps/apple"
log_dir="${TMPDIR:-/tmp}/vaultday-gates/$(basename "$repo_root")"
failure_tail_lines=40

mode=""
base_ref="origin/main"
verbose=false
while [[ $# -gt 0 ]]; do
	case "$1" in
		--full) mode=full ;;
		--changed) mode=changed ;;
		--base)
			base_ref="${2:?--base needs a ref}"
			shift
			;;
		--verbose) verbose=true ;;
		*)
			mode=""
			break
			;;
	esac
	shift
done
if [[ -z "$mode" ]]; then
	echo "usage: scripts/gates.sh --full | --changed [--base <ref>] [--verbose]" >&2
	exit 64
fi

rm -rf "$log_dir"
mkdir -p "$log_dir"

gate_names=()
gate_seconds=()
gate_results=()
has_failure=false

# Runs one gate, logs its output and records its time and result.
function run_gate() {
	local name="$1"
	shift
	local log_file="$log_dir/$name.log"
	local started=$SECONDS
	local result=passed
	echo "==> $name"
	if [[ "$verbose" == true ]]; then
		"$@" 2>&1 | tee "$log_file" || result=failed
	else
		"$@" >"$log_file" 2>&1 || result=failed
	fi
	local elapsed=$((SECONDS - started))
	gate_names+=("$name")
	gate_seconds+=("$elapsed")
	gate_results+=("$result")
	if [[ "$result" == failed ]]; then
		has_failure=true
		echo "    FAILED after ${elapsed}s; last lines of $log_file:"
		tail -n "$failure_tail_lines" "$log_file" | sed 's/^/    /'
		return 1
	fi
	echo "    passed in ${elapsed}s"
}

function skip_gate() {
	gate_names+=("$1")
	gate_seconds+=("-")
	gate_results+=("skipped: $2")
}

function print_summary() {
	echo
	printf '%-18s %8s  %s\n' gate seconds result
	local index
	for index in "${!gate_names[@]}"; do
		printf '%-18s %8s  %s\n' "${gate_names[$index]}" "${gate_seconds[$index]}" "${gate_results[$index]}"
	done
	printf '%-18s %8s\n' total "$((SECONDS - run_started))"
}

# --- Rust ---------------------------------------------------------------------

function run_rust_gates() {
	local package_flags=("$@")
	run_gate cargo-fmt bash -c "cd '$core_dir' && cargo fmt --all --check"
	run_gate cargo-clippy bash -c "cd '$core_dir' && cargo clippy ${package_flags[*]:-} --all-targets --all-features -- -D warnings"
	if cargo nextest --version >/dev/null 2>&1; then
		run_gate cargo-nextest bash -c "cd '$core_dir' && cargo nextest run ${package_flags[*]:-} --all-features"
		# nextest does not run doc tests.
		run_gate cargo-doctest bash -c "cd '$core_dir' && cargo test ${package_flags[*]:-} --all-features --doc"
	else
		run_gate cargo-test bash -c "cd '$core_dir' && cargo test ${package_flags[*]:-} --all-features"
	fi
}

# --- Apple --------------------------------------------------------------------

function run_apple_gates() {
	if ! run_gate xcframework "$repo_root/scripts/build-xcframework.sh"; then
		skip_gate xcodebuild-test 'the XCFramework did not build'
		return
	fi
	run_gate xcodegen bash -c "cd '$apple_dir' && xcodegen generate --quiet"
	# One Swift compile: the scheme's test action holds the app tests and the
	# VaultdayCore package tests. Debug only, for the machine's own
	# architecture, without the index store Xcode's editor uses, into a
	# DerivedData folder that is kept between runs.
	run_gate xcodebuild-test bash -c "cd '$apple_dir' && xcodebuild test \
		-project Vaultday.xcodeproj -scheme Vaultday -configuration Debug \
		-destination 'platform=macOS,arch=$(uname -m)' -derivedDataPath build \
		-skipPackagePluginValidation -skipMacroValidation -parallelizeTargets -quiet \
		CODE_SIGNING_ALLOWED=NO ONLY_ACTIVE_ARCH=YES \
		COMPILER_INDEX_STORE_ENABLE=NO DEBUG_INFORMATION_FORMAT=dwarf"
}

# --- Node ---------------------------------------------------------------------

function run_node_gates() {
	local filter_flags=("$@")
	run_gate pnpm-install bash -c "cd '$repo_root' && pnpm install --frozen-lockfile --prefer-offline"
	# The CLI's types and tests load the napi binding; a debug build reuses
	# the crates `cargo test` already compiled.
	if ! run_gate napi-binding bash -c "cd '$repo_root' && pnpm --filter @vaultday/core-node build:debug"; then
		skip_gate node-typecheck 'the napi binding did not build'
		skip_gate vitest 'the napi binding did not build'
		return
	fi
	if [[ ${#filter_flags[@]} -eq 0 ]]; then
		run_gate node-typecheck bash -c "cd '$repo_root' && pnpm -r typecheck"
		run_gate vitest bash -c "cd '$repo_root' && pnpm -r test"
	else
		run_gate node-typecheck bash -c "cd '$repo_root' && pnpm ${filter_flags[*]} run --if-present typecheck"
		run_gate vitest bash -c "cd '$repo_root' && pnpm ${filter_flags[*]} run --if-present test"
	fi
}

function run_biome_gate() {
	run_gate biome bash -c "cd '$repo_root' && pnpm lint"
}

function run_keymap_gate() {
	run_gate keymap-doc bash -c "cd '$repo_root' && pnpm --filter @vaultday/keymap-doc check"
}

function run_tokens_gate() {
	run_gate tokens bash -c "cd '$repo_root' && pnpm --filter @vaultday/tokens check"
}

# --- Modes --------------------------------------------------------------------

function run_full() {
	run_rust_gates
	run_apple_gates
	run_node_gates
	run_biome_gate
	run_tokens_gate
	run_keymap_gate
}

function run_changed() {
	local merge_base
	if ! merge_base="$(git -C "$repo_root" merge-base HEAD "$base_ref")"; then
		echo "cannot find the merge base of HEAD and $base_ref (git fetch first?)" >&2
		exit 1
	fi
	local changed_files
	changed_files="$(list_changed_files "$merge_base")"
	if [[ -z "$changed_files" ]]; then
		echo "Nothing changed since $base_ref ($merge_base)."
		return
	fi
	echo "Changed since $base_ref ($(git -C "$repo_root" rev-parse --short "$merge_base")): $(echo "$changed_files" | wc -l | tr -d ' ') files"

	local crates
	crates="$(list_affected_crates "$changed_files")"
	if [[ "$crates" == all ]]; then
		run_rust_gates
	elif [[ -n "$crates" ]]; then
		echo "Rust crates: $(echo $crates)"
		local package_flags=()
		local crate
		for crate in $crates; do
			package_flags+=(-p "$crate")
		done
		run_rust_gates "${package_flags[@]}"
	else
		skip_gate rust 'no core changes'
	fi

	# Markdown under apps/apple is documentation, not app input.
	if echo "$changed_files" | grep -v '\.md$' |
		grep -Eq '^(apps/apple/|core/crates/(ffi|uniffi-bindgen)/|scripts/build-xcframework\.sh$|scripts/env\.sh$)'; then
		run_apple_gates
	else
		skip_gate apple 'no apps/apple or FFI changes'
	fi

	local node_filters
	node_filters="$(list_node_filters "$changed_files")"
	# A Rust change reaching the napi crate changes what the CLI loads.
	if [[ "$node_filters" != all ]] && { [[ "$crates" == all ]] || echo "$crates" | grep -qx vaultday-node; }; then
		node_filters="$node_filters--filter '...{./core/crates/node}' "
	fi
	if [[ "$node_filters" == all ]]; then
		run_node_gates
	elif [[ -n "$node_filters" ]]; then
		# Word splitting is intended: one `--filter` flag per changed package.
		# shellcheck disable=SC2086
		run_node_gates $node_filters
	else
		skip_gate node 'no cli, tools or core/crates/node changes'
	fi

	if echo "$changed_files" | grep -Eq '\.(ts|tsx|js|mjs|cjs|json|jsonc|css)$'; then
		run_biome_gate
	else
		skip_gate biome 'no files biome reads'
	fi

	if echo "$changed_files" | grep -Eq '^(apps/apple/Keymap\.yaml|tools/keymap-doc/|apps/apple/KEYMAP\.md)'; then
		run_keymap_gate
	fi
	if echo "$changed_files" | grep -Eq '^(design/tokens\.json|tools/tokens/|apps/apple/Vaultday/Tokens\.swift)'; then
		run_tokens_gate
	fi
}

# --- Helpers ------------------------------------------------------------------

# Repo-relative paths changed since the merge base: committed, staged,
# unstaged and untracked.
function list_changed_files() {
	local merge_base="$1"
	{
		git -C "$repo_root" diff --name-only "$merge_base"
		git -C "$repo_root" ls-files --others --exclude-standard
	} | sort -u
}

# The workspace crates to check: those owning a changed file plus their
# dependents, as package names; `all` when a workspace-wide input changed.
function list_affected_crates() {
	local changed_files="$1"
	if echo "$changed_files" | grep -Eq '^(core/Cargo\.(toml|lock)|rust-toolchain\.toml|core/fixtures/|core/\.cargo/)'; then
		echo all
		return
	fi
	local core_changes
	core_changes="$(echo "$changed_files" | grep '^core/crates/' || true)"
	if [[ -z "$core_changes" ]]; then
		return
	fi
	local metadata
	metadata="$(cd "$core_dir" && cargo metadata --format-version 1 --no-deps --offline 2>/dev/null ||
		cargo metadata --format-version 1 --no-deps)"
	echo "$metadata" | jq -r --arg root "$repo_root/" --arg changed "$core_changes" '
		[.packages[] | {
			name,
			directory: (.manifest_path | ltrimstr($root) | rtrimstr("Cargo.toml")),
			dependencies: [.dependencies[] | select(.path) | .name],
		}] as $packages
		| ($changed | split("\n") | map(select(length > 0))) as $files
		| [$packages[] | select(.directory as $directory | any($files[]; startswith($directory))) | .name] as $owners
		| def with_dependents($set):
			([$packages[] | select(any(.dependencies[]; . as $dependency | $set | index($dependency))) | .name] + $set | unique) as $grown
			| if ($grown | length) == ($set | length) then $set else with_dependents($grown) end;
		with_dependents($owners) | .[]'
}

# `--filter` flags selecting the changed pnpm packages and their dependents;
# `all` when a root manifest changed.
function list_node_filters() {
	local changed_files="$1"
	if echo "$changed_files" | grep -Eq '^(package\.json|pnpm-lock\.yaml|pnpm-workspace\.yaml|tsconfig\.base\.json|\.nvmrc)$'; then
		echo all
		return
	fi
	echo "$changed_files" | grep -Eo '^(cli|core/crates/node|tools/[^/]+)/' | sort -u |
		while read -r package_directory; do
			if [[ -f "$repo_root/${package_directory}package.json" ]]; then
				printf -- "--filter '...{./%s}' " "${package_directory%/}"
			fi
		done
}

run_started=$SECONDS
if [[ "$mode" == full ]]; then
	run_full
else
	run_changed
fi
print_summary
if [[ "$has_failure" == true ]]; then
	exit 1
fi
