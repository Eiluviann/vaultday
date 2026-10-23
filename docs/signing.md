# Signing the Mac app without the paid membership

Vaultday's Mac app is sandboxed, keeps its keys in the Keychain and its state outside the vault. Without the paid Apple Developer Program (99 USD / 100 EUR a year) it still persists between launches. Two routes work:

- **Personal Team** (recommended, needs one sign-in): a free Apple ID in Xcode.
  - The build signs with identifiers of its own, so the free team never registers the shipped bundle id or App Group.
  - Keys go to the data protection keychain and survive rebuilds without prompts.
  - State goes to a team-prefixed App Group container.
- **Ad hoc** (the default with no setup, "Sign to Run Locally"): keys go to the login keychain. It persists, but after every rebuild macOS asks once per Keychain item before the new build may read it.

What each signing gives up is decided by the entitlements the build claims, one at a time (`SigningMode` in `VaultdayCore/Environment/SigningMode.swift`). The app never falls back to a throwaway run just because it lacks one entitlement.

## Recipe: Personal Team

1. Xcode › Settings › Accounts › **+** › Apple Account. Sign in with a plain Apple ID. Xcode creates a team named "<Your Name> (Personal Team)".
2. In the same pane, select the team and choose **Manage Certificates… › + › Apple Development**. Skip this if Xcode already made one.
3. Check that the certificate is a valid signing identity:

   ```bash
   security find-identity -v -p codesigning
   ```

   It must list "Apple Development: …" under the valid identities. If it shows `0 valid identities found` while `security find-identity -p codesigning` (without `-v`) lists the certificate, the chain is incomplete: the Apple Worldwide Developer Relations G3 intermediate that issued it is missing.

   - Download "Worldwide Developer Relations - G3" from https://www.apple.com/certificateauthority/.
   - Add it to the login keychain with `security add-certificates AppleWWDRCAG3.cer`.
   - Change no trust settings: the intermediate chains to the Apple Root CA, which macOS already trusts.
   - Old Macs often hold only the expired original WWDR certificate (2023) in the System keychain, which does not count.
4. Find the team id: it is the Organizational Unit of the new certificate, not the 10 characters in its name, which are the member id.

   ```bash
   security find-certificate -c "Apple Development" -p | openssl x509 -noout -subject
   ```

   `OU=ABCDE12345` is the team id.
5. Write `apps/apple/Signing.local.xcconfig`. The file is gitignored.

   ```
   DEVELOPMENT_TEAM = ABCDE12345
   #include "Signing.personal-team.xcconfig"
   ```

   `Signing.personal-team.xcconfig` (committed) signs with the full entitlement set and swaps in a set of identifiers of its own:

   | | Shipped app (paid team, later) | Personal Team build |
   | --- | --- | --- |
   | Bundle id | `com.eiluviann.vaultday` | `com.eiluviann.vaultday.personal` |
   | App Group | `group.com.eiluviann.vaultday` | `<TEAM>.com.eiluviann.vaultday` (macOS-style, never registered: `REGISTER_APP_GROUPS = NO`) |
   | Keychain access group | the App Group | `<TEAM>.com.eiluviann.vaultday.personal` (the `keychain-access-groups` entry) |

   - **Why own identifiers.** Bundle ids and `group.` App Groups are unique across all teams, and Xcode registers what it signs. If the personal team signed with the shipped names, it would hold them, and the paid team could not register them later.
   - **Why a separate Keychain group.** A team-prefixed App Group opens without a prompt on macOS 15 but is not a Keychain access group. The items therefore live in the `keychain-access-groups` entry, which the team's profile grants (`<TEAM>.*`).

6. Build. `-allowProvisioningUpdates` lets `xcodebuild` register the App ID and make the Mac development profile. In the Xcode app this happens by itself.

   ```bash
   cd apps/apple && xcodegen generate && xcodebuild -project Vaultday.xcodeproj -scheme Vaultday -configuration Debug -derivedDataPath build -allowProvisioningUpdates build
   ```

7. Run `apps/apple/build/Build/Products/Debug/Vaultday.app`. The launch log says `signing mode appGroup("ABCDE12345.com.eiluviann.vaultday")`, and the sidebar shows no signing notice.

   The app is a separate app from the shipped one: its own bundle id, sandbox container, privacy grants and Keychain items.

**Fallback if the App Group does not open or the Keychain refuses it.** Add `VAULTDAY_ENTITLEMENTS = Signing/PersonalTeam.entitlements` after the `#include` line. That set drops the App Group.

- Keys stay in the team's Keychain group, device-only.
- State goes to `~/Library/Application Support/Vaultday/<TEAM>.com.eiluviann.vaultday.personal`.
- The sidebar says "Signed without the App Group: …".

**Rebuild weekly.** Free-account App IDs and profiles expire after 7 days [compare memberships]. A Personal Team build needs a rebuild about once a week, signed in to Xcode, which renews the profile. Keys and state survive the rebuild: the certificate-based designated requirement stays the same.

**iCloud Keychain.** Synchronisable Keychain items need only a valid access group, no iCloud entitlement. This comes from the parallel research session. It agrees with [kSecAttrSynchronizable doc] and [TN3137], which ask for the data protection keychain and `keychain-access-groups`, nothing more. A Personal Team build in `appGroup` mode therefore synchronises data keys like the shipped app. This was not verified here, since nobody was signed in. If `SecItemAdd` refuses a synchronisable item, the app says so at launch, and the `PersonalTeam.entitlements` fallback keeps every item device-only.

**Coming from the old recipe.** A `Signing.local.xcconfig` that holds only `DEVELOPMENT_TEAM = …` now builds ad hoc, because the defaults sign with `-` and `AdHoc.entitlements`. Such a build runs in login-keychain mode, beside the old state, which stays where it was. Add the `#include` line (Personal Team), or `CODE_SIGN_IDENTITY = Apple Development` with `VAULTDAY_ENTITLEMENTS = Signing/Full.entitlements` (paid team).

## Recipe: ad hoc (no Apple ID)

Nothing to configure. With no `Signing.local.xcconfig`, `Signing.xcconfig` signs with `CODE_SIGN_IDENTITY = -` and `Signing/AdHoc.entitlements`. Build with the command above, without `-allowProvisioningUpdates`.

- **Keychain prompts.** Each new build is a different program to the login keychain, because its designated requirement is its cdhash. The first launch of a rebuilt app asks "Vaultday wants to use your confidential information…" once per item (8 items with one workspace).
  - Answer with your login password and **Always Allow**. Relaunching the same build asks nothing.
  - An explicit `-r 'designated => identifier …'` does not avoid the prompts; this was tried.
- **Calendar and Reminders.** The privacy prompts also come back after a rebuild, for the same reason (TN3127).
- **Folder grants after a rebuild.** Whether the security-scoped bookmarks of picked folders still resolve after a rebuild was not tested. If a workspace shows as unavailable after a rebuild, pick its folder again.

An unsigned build (`CODE_SIGNING_ALLOWED=NO`) claims no entitlements and runs unsandboxed. It gets the same login-keychain mode and the same folder.

## Which mode the app runs in

| Mode | Signed by | Entitlements file | Keychain | Sync to other devices | App state | API files (CLI) |
| --- | --- | --- | --- | --- | --- | --- |
| `appGroup` (shipped ids) | paid team | `Signing/Full.entitlements` | data protection, access group `group.com.eiluviann.vaultday` | yes, data keys through iCloud Keychain | `~/Library/Group Containers/group.com.eiluviann.vaultday` | `…/Group Containers/group.com.eiluviann.vaultday/API` |
| `appGroup` (personal ids) | Personal Team, `Signing.personal-team.xcconfig` | `Signing/Full.entitlements` | data protection, access group `<TEAM>.com.eiluviann.vaultday.personal` | yes (unverified here) | `~/Library/Group Containers/<TEAM>.com.eiluviann.vaultday` | `…/Group Containers/<TEAM>.com.eiluviann.vaultday/API` |
| `teamKeychain` | Personal Team, fallback | `Signing/PersonalTeam.entitlements` | data protection, access group `<TEAM>.com.eiluviann.vaultday.personal` | no, every item device-only | `~/Library/Application Support/Vaultday/<TEAM>.com.eiluviann.vaultday.personal` | `~/Library/Application Support/Vaultday/API` |
| `loginKeychain` | ad hoc, or unsigned | `Signing/AdHoc.entitlements` (or none) | file-based login keychain | no, every item device-only | `~/Library/Application Support/Vaultday/login-keychain` | `~/Library/Application Support/Vaultday/API` |
| `inMemory` | Debug, when no keychain answers | — | in memory | no | a fresh temporary folder per run | same folder |

The mode follows from what the signature claims, read through `SecTask`, so it is the same at every launch of one build:

- A claim on the App Group that Info.plist names (`VAULTDAY_APP_GROUP`) gives `appGroup`.
- Otherwise a concrete `keychain-access-groups` entry gives `teamKeychain`.
- Otherwise the app uses `loginKeychain`.

The App Group and team-keychain modes then write and delete a probe item in the claimed Keychain group, since an unentitled read only seems to succeed. If the write is refused, they stop with "…its provisioning profile does not grant it" rather than degrade silently.

The in-memory mode is taken only when there is no login keychain at all (`errSecNoSuchKeychain`, `errSecNotAvailable`). A locked or refused keychain stops with its status instead, so one build never alternates between its state and a throwaway run.

How to tell which mode a build runs in:

- **Sidebar.** Every mode except `appGroup` shows its notice at the foot of the sidebar, with the state folder.
- **Log.** Every launch logs the mode:

  ```bash
  log stream --style compact --predicate 'subsystem == "com.eiluviann.vaultday" AND category == "signing"'
  ```

  It prints, for example, `signing mode loginKeychain, state in /Users/you/Library/Application Support/Vaultday/login-keychain`.
- **Signature.** What the build claims:

  ```bash
  codesign -d --entitlements - --xml apps/apple/build/Build/Products/Debug/Vaultday.app | plutil -p -
  ```

**Switching.** App state opens only with the keys of the keychain that wrote it. Each keychain therefore gets its own state folder, and switching between ad hoc, Personal Team and the full set starts that mode fresh. Nothing is migrated: add the workspaces again. Each mode's old state stays where it was. The vault is never touched by any of this.

**The CLI** looks in the shipped App Group's `API` folder first, then in any `<TEAM>.com.eiluviann.vaultday` group container, then in `~/Library/Application Support/Vaultday/API`. It uses the first folder, in that order, that holds both `token` and `port` and whose server answers the `/v1/hello` proof for that token; the app removes `port` when it quits, and a port a crashed build left behind is skipped because nothing answers on it. The order is a trust order, not file age: files planted in the unprotected Application Support folder never outrank a running App Group build. `VAULTDAY_API_DIR` still overrides all of them.

If the sandbox container path is ever used instead (when the folder exception is missing), the CLI does not look there. Set `VAULTDAY_API_DIR` to `~/Library/Containers/com.eiluviann.vaultday/Data/Library/Application Support/Vaultday/API`.

**Security trade-off of the local folder.** The App Group container is protected from other apps. `~/Library/Application Support/Vaultday` is a plain folder that any process running as you can read. It is reached through the sandbox's `temporary-exception.files.home-relative-path.read-write` entitlement, and only the reduced entitlement sets claim it.

- The app state there is encrypted with keys from the Keychain.
- In login-keychain mode those keys have no access group, so any process running as you could plant an item under Vaultday's service names before the first launch. The app would then adopt it. This is the same "any process of yours" trust as the token file. A Personal Team build's data protection items are bound to its Keychain group.
- The API token file is mode 0600, as before.
- The token's only use is the loopback API, which already trusts any process of yours.

## What each entitlement needs (step-1 findings)

Legend: ✅ works · ⚠️ works with a caveat · ❌ not available. "Verified" means it was run on this Mac (macOS 26, Xcode 26.6, October 2026). Everything else comes from the sources below.

| Entitlement | Personal Team | Ad hoc (`codesign -s -`) |
| --- | --- | --- |
| `com.apple.security.app-sandbox` | ✅ unrestricted [TN3125] | ✅ verified: sandboxed, `NSHomeDirectory()` is the container |
| `files.user-selected.read-write` + `files.bookmarks.app-scope` | ✅ unrestricted [TN3125] | ⚠️ verified within one build: the workspace reopened from its bookmark after a relaunch. Across a rebuild: untested |
| `network.server` | ✅ unrestricted | ✅ verified: the loopback API answered the CLI |
| `network.client` | ✅ unrestricted | ✅ unrestricted (not exercised: nothing may contact live services) |
| `personal-information.calendars` / `.reminders` | ✅ unrestricted; the privacy grant survives rebuilds (stable DR) [TN3127, forum 678816] | ⚠️ honoured, but the privacy prompt comes back after each rebuild [TN3127, forum 678816] |
| `keychain-access-groups` | ✅ "Keychain sharing" is in the free column [capabilities table]; restricted, so it needs the profile Xcode makes, which grants `<TEAM>.*` [TN3125, TN3137] | ❌ restricted: no profile, and AMFI refuses to launch a build that claims it [forum 706427, 787827]. `AdHoc.entitlements` leaves it out |
| Data protection keychain at all | ✅ with the App ID the profile carries [forum 740164] | ❌ verified: `SecItemAdd` answers -34018 (`errSecMissingEntitlement`), even without an access group; reads only seem to work (`errSecItemNotFound`) |
| File-based login keychain | not used | ✅ verified: items written, read back after a relaunch, prompt after a rebuild [TN3137, forum 98484] |
| `kSecAttrSynchronizable` (iCloud Keychain) | ⚠️ needs the data protection keychain and an access group [kSecAttrSynchronizable doc, TN3137]. Whether a free team's items actually reach another device is unsourced and unverified. In `appGroup` mode Vaultday writes data keys synchronisable, as the shipped app does; the `PersonalTeam.entitlements` fallback (`teamKeychain`) keeps every item device-only | ❌ the file-based keychain never synchronises; verified: a query with it finds nothing |
| `com.apple.security.application-groups` | ✅ (unverified here) "App groups" is in the free column [capabilities table]. A `group.` id must be registered and authorised by the profile. A `<TEAM>.` id needs neither and opens without the macOS 15 prompt [forum 721701, entitlement doc], hence `<TEAM>.com.eiluviann.vaultday` in `Signing.personal-team.xcconfig`. It is not a Keychain access group. Fallback without it: `PersonalTeam.entitlements` | ❌ in practice: the entitlement is unrestricted, but without a team id or profile, macOS 15+ asks for consent at each launch before the container opens [forum 721701]. `containerURL(…)` returns a path even when not entitled [containerURL doc; verified] |
| Secure Enclave keys (CryptoKit) | ✅ | ✅ verified: a P-256 key was made in the sandboxed ad-hoc probe |
| `temporary-exception.files.home-relative-path.read-write` | ✅ sandbox configuration, unrestricted [TN3125] | ✅ verified: writes to `~/Library/Application Support/Vaultday` |

Notes:

- **Restricted vs unrestricted [TN3125].**
  - Honoured on any signature: the App Sandbox entitlements and the ones that configure it, `application-groups`, `get-task-allow`, and the hardened-runtime entitlements.
  - Everything else needs a provisioning profile: `keychain-access-groups`, `com.apple.application-identifier`, `com.apple.developer.*`.
  - A build that claims a restricted entitlement without a profile is killed at launch. taskgated logs "Unsatisfied entitlements", and amfid logs "Restricted entitlements not validated" [forum 706427, 787827].
- **Personal Team** gets an Apple Development certificate. When a restricted entitlement is claimed, it also gets a Mac development profile limited to this Mac; DTS confirmed this with a free account on macOS 15 [forum 787500].
  - Not available to free accounts on macOS: iCloud (CloudKit, documents, key-value), Push, Associated Domains, and the rest of the ADP-only rows [capabilities table].
  - App IDs and profiles expire after 7 days [compare memberships], hence the weekly rebuild.
  - Never sign the shipped identifiers with it: bundle ids and `group.` App Groups are unique across all teams, and the personal team would keep them.
  - The team id is 10 characters [forum 811970].
- **Not pursued: a self-signed code-signing certificate.** A certificate made in Keychain Access (Certificate Assistant › Create a Certificate › Code Signing) would give ad-hoc-like builds a stable designated requirement, with no Keychain or privacy re-prompts after rebuilds. It still has no team or profile, so the data protection keychain stays out of reach.
  - `codesign` refused an untrusted test certificate in a temporary keychain.
  - Making it usable means trusting it in your keychain settings, which is yours to decide. The Personal Team gives the same benefit without it.

## Verified on this Mac (2026-10-23)

### Personal Team

- **Account.** The user signed in an Apple ID and made the Apple Development certificate. The team id came from the certificate's OU.
- **`find-identity -v`.** It first reported 0 *valid* identities while the WWDR G3 intermediate was missing from the login keychain. Once G3 was in the login keychain (with G5 and G6), it reported 1 valid identity. `codesign` signed with the certificate, and `security verify-cert -p codeSign` passed.
- **Build.** `xcodebuild -allowProvisioningUpdates` with `DEVELOPMENT_TEAM = <TEAM>` and the `Signing.personal-team.xcconfig` include made "Mac Team Provisioning Profile: com.eiluviann.vaultday.personal".
  - Profile: `LocalProvision`, this Mac only, `keychain-access-groups` `<TEAM>.*`, expires 7 days after issue.
  - Signature claims: `com.apple.application-identifier` `<TEAM>.com.eiluviann.vaultday.personal`, App Group `<TEAM>.com.eiluviann.vaultday`, Keychain group `<TEAM>.com.eiluviann.vaultday.personal`.
  - Designated requirement: `identifier … and anchor apple generic and certificate leaf[subject.CN] = "Apple Development: …"`, which stays the same across rebuilds.
- **Launch.** Log: `signing mode appGroup("<TEAM>.com.eiluviann.vaultday")`, state in `~/Library/Group Containers/<TEAM>.com.eiluviann.vaultday`, with no container prompt.
  - The synchronisable data keys were written without error, so `kSecAttrSynchronizable` is accepted under a Personal Team. Whether iCloud carries them to another device was not checked.
- **Persistence.** The workspace came from a mock-vault copy inside that container. `vaultday timer start` went through the team-prefixed `API` folder. After a quit and a relaunch:
  - The same token (`7e7f612549f2d444…`) and the same AppState snapshot came back.
  - The workspace reopened, and the timer was still running.

### Ad hoc

- **Starting point.** No signing identity on this Mac (`security find-identity -v -p codesigning`: 0), no team in Xcode.
- **Ad-hoc build.** The default build with `Signing/AdHoc.entitlements` succeeded ("Sign to Run Locally").
- **First launch.** The build was launched against a copy of `core/fixtures/mock-vault` (`VAULTDAY_DEBUG_WORKSPACE`).
  - Log: `signing mode loginKeychain, state in ~/Library/Application Support/Vaultday/login-keychain`.
  - The app wrote one AppState snapshot and `API/token`, mode 0600 in a 0700 folder.
  - `vaultday status` found the app through the new fallback folder.
  - `vaultday timer start task-2 --ago 5` started a timer.
- **Relaunch.** The app was quit (SIGTERM) and relaunched without `VAULTDAY_DEBUG_WORKSPACE`. Afterwards:
  - The same token (SHA-256 prefix `42b06a8606cce6f9`) was rewritten from the Keychain.
  - The same AppState snapshot opened.
  - The workspace "Mock Studio" was still there.
  - `vaultday status --json` showed the timer `ses-1` still running at 5 min 39 s.
  - The keys, the token and the state survived the restart.
- **Cleanup.** The test state and its 8 login-keychain items were removed afterwards.
- **Probes.** A minimal sandboxed probe app gave the data-protection, file-keychain, rebuild-prompt, Secure Enclave, folder-exception and App Group results in the table above.

## Sources

- [capabilities table] Apple, Supported capabilities (macOS): https://developer.apple.com/help/account/reference/supported-capabilities-macos/. Columns ADP / Developer ID / Apple Developer; the last is the free account.
- [compare memberships] Apple, Choosing a membership: https://developer.apple.com/support/compare-memberships/
- [TN3125] Inside Code Signing: Provisioning Profiles: https://developer.apple.com/documentation/technotes/tn3125-inside-code-signing-provisioning-profiles
- [TN3127] Inside Code Signing: Requirements: https://developer.apple.com/documentation/technotes/tn3127-inside-code-signing-requirements
- [TN3137] On Mac keychain APIs and implementations: https://developer.apple.com/documentation/technotes/tn3137-on-mac-keychains
- [entitlement doc] App Groups entitlement: https://developer.apple.com/documentation/bundleresources/entitlements/com.apple.security.application-groups
- [containerURL doc] https://developer.apple.com/documentation/foundation/filemanager/containerurl(forsecurityapplicationgroupidentifier:)
- [kSecAttrSynchronizable doc] https://developer.apple.com/documentation/security/ksecattrsynchronizable
- Apple Developer Forums, posts by DTS:
  - [forum 787500] Personal Team profile on macOS: https://developer.apple.com/forums/thread/787500
  - [forum 721701] App Groups: macOS vs iOS: https://developer.apple.com/forums/thread/721701
  - [forum 740164] data protection keychain needs an App ID: https://developer.apple.com/forums/thread/740164
  - [forum 98484] file-based keychain ACLs and the DR: https://developer.apple.com/forums/thread/98484
  - [forum 678816] privacy grants and rebuilds: https://developer.apple.com/forums/thread/678816
  - [forum 706427] and [forum 787827] restricted entitlements at launch: https://developer.apple.com/forums/thread/706427, https://developer.apple.com/forums/thread/787827
  - [forum 811970] team id format: https://developer.apple.com/forums/thread/811970
