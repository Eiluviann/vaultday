#!/usr/bin/env bash
# A sketchybar item for the Vaultday timer. sketchybar runs it every
# `update_freq` seconds and on a click; it polls `vaultday status --format
# sketchybar` and sets the item's label. Register it in sketchybarrc:
#
#   sketchybar --add item vaultday right \
#              --set vaultday update_freq=2 \
#                    script="$PLUGIN_DIR/vaultday.sh" \
#                    click_script="$PLUGIN_DIR/vaultday.sh click"
#
# A click stops the running timer, or continues the latest one when none runs.
# The output is read as key=value lines and never evaluated as shell.
set -u

vaultday_bin="${VAULTDAY_BIN:-vaultday}"
item="${NAME:-vaultday}"

if [[ "${1:-}" == "click" || "${SENDER:-}" == "mouse.clicked" ]]; then
	if [[ "$("$vaultday_bin" status --format sketchybar 2>/dev/null | grep '^state=')" == "state=idle" ]]; then
		"$vaultday_bin" timer continue >/dev/null 2>&1
	else
		"$vaultday_bin" timer stop >/dev/null 2>&1
	fi
fi

if ! status="$("$vaultday_bin" status --format sketchybar 2>/dev/null)"; then
	# The app is not running, or refused the token: hide the item.
	sketchybar --set "$item" drawing=off
	exit 0
fi

state=""
label=""
ring=""
while IFS='=' read -r key value; do
	case "$key" in
		state) state="$value" ;;
		label) label="$value" ;;
		ring) ring="$value" ;;
	esac
done <<<"$status"

case "$state" in
	running) sketchybar --set "$item" drawing=on icon="●" label="$label" ;;
	paused) sketchybar --set "$item" drawing=on icon="❙❙" label="$label" ;;
	*) sketchybar --set "$item" drawing=on icon="○" label="$ring" ;;
esac
