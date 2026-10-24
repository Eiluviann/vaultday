/** The root folder names of the two playground workspaces. */
export type PlaygroundFolders = {
	work: string;
	life: string;
};

/** The double-clickable command's file name in the playground folder. */
export const LAUNCHER_NAME = 'Open Vaultday Playground.command';

/** Where the signed Personal Team build is installed (`docs/signing.md`). */
const INSTALLED_APP = '$HOME/Applications/Vaultday.app';

/**
 * The `.command` that opens the playground: it checks both roots are there,
 * opens a Finder window on them (to drag into the open panel) and the
 * installed app, and prints the two imports left to do. It writes nothing:
 * the sandboxed app takes a folder only from its open panel.
 */
export function launcherScript(folders: PlaygroundFolders): string {
	const folderList = [folders.work, folders.life]
		.map((folder) => `"${folder}"`)
		.join(' ');
	return `#!/bin/bash
# Opens Vaultday on the playground. Written by tools/playground (scripts/playground.sh), which also resets it.
set -euo pipefail

playground="$(cd "$(dirname "$0")" && pwd)"
app="${INSTALLED_APP}"

for root in ${folderList}; do
	if [[ ! -f "$playground/$root/.vaultday/settings.json" ]]; then
		echo "$playground/$root holds no workspace. Run scripts/playground.sh in the vaultday repository first."
		exit 1
	fi
done
if [[ ! -d "$app" ]]; then
	echo "Vaultday is not installed at $app (see docs/signing.md)."
	exit 1
fi

open "$playground"
open -a "$app"

cat <<STEPS

Vaultday is opening. The sandbox lets it read a folder only once you pick it
in its open panel, so two imports are left (each folder already holds its
.vaultday settings, so nothing needs filling in):

  1. If the setup is not on screen: Vaultday menu > Set up Vaultday...
     and go to "Your workspaces".
  2. Work > Add > Import a workspace, pick
       $playground/${folders.work}
     (drag it from the Finder window into the panel, or press Cmd-Shift-G
     and paste the path), click Import, then Done on the Configure sheet.
  3. Life > Add > Import a workspace, pick
       $playground/${folders.life}
     the same way, then Done.

You only do this once: the app keeps both folders from then on, and a reset
(scripts/playground.sh) rewrites the notes without unregistering them.
STEPS
`;
}
