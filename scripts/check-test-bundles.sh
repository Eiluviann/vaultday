#!/usr/bin/env bash
# Fails unless the newest `xcodebuild test` result under a DerivedData folder
# ran tests from every bundle the run should hold: the app tests
# (VaultdayTests), the VaultdayCore package tests (VaultdayCoreTests) and, on
# CI (`CI` set), the window tests (VaultdayWindowTests), which local runs skip.
#
# A scheme that loses the package test target (a renamed target, a wrong
# `package:` entry in project.yml) still passes `xcodebuild test` with the app
# tests alone, and xcodegen does not complain either. CI runs the package
# tests only through the scheme, so this keeps them from vanishing quietly.
#
# Usage: scripts/check-test-bundles.sh <derived-data-path> [<bundle>...]
# (the bundles default to those above)
set -euo pipefail

derived_data="${1:?usage: scripts/check-test-bundles.sh <derived-data-path> [<bundle>...]}"
shift
expected_bundles=("$@")
if [[ ${#expected_bundles[@]} -eq 0 ]]; then
	expected_bundles=(VaultdayTests VaultdayCoreTests)
	if [[ -n "${CI:-}" ]]; then
		expected_bundles+=(VaultdayWindowTests)
	fi
fi

result_bundle="$(ls -td "$derived_data"/Logs/Test/*.xcresult 2>/dev/null | head -n 1 || true)"
if [[ -z "$result_bundle" ]]; then
	echo "no test result under $derived_data/Logs/Test" >&2
	exit 1
fi

# One line per bundle: its name and the number of test cases that ran in it.
bundle_counts="$(xcrun xcresulttool get test-results tests --path "$result_bundle" |
	jq -r '.. | objects | select(.nodeType == "Unit test bundle")
		| "\(.name) \([.. | objects | select(.nodeType == "Test Case")] | length)"')"

has_missing_bundle=false
for bundle in "${expected_bundles[@]}"; do
	count="$(echo "$bundle_counts" | awk -v name="$bundle" '$1 == name { sum += $2 } END { print sum + 0 }')"
	if [[ "$count" -eq 0 ]]; then
		echo "$bundle ran no tests in $result_bundle" >&2
		has_missing_bundle=true
	else
		echo "$bundle: $count tests"
	fi
done
[[ "$has_missing_bundle" == false ]]
