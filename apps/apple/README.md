# Vaultday for Mac

The SwiftUI app (`Vaultday/`), its tests (`VaultdayTests/`) and the `VaultdayCore` Swift package over the Rust core's XCFramework. The Xcode project is generated: `project.yml` is the source, `Vaultday.xcodeproj` is gitignored.

```bash
scripts/build-xcframework.sh   # from the repo root: the XCFramework and the generated Swift
cd apps/apple && xcodegen generate
xcodebuild -project Vaultday.xcodeproj -scheme Vaultday -configuration Debug -derivedDataPath build build test
```

## Signing

Without any setup the app is signed ad hoc and keeps its keys in the login keychain and its state in `~/Library/Application Support/Vaultday`. With a free Personal Team (an Apple ID in Xcode) the build signs with identifiers of its own (`Signing.personal-team.xcconfig`: `com.eiluviann.vaultday.personal`, a team-prefixed App Group), keys go to the data protection keychain and rebuilds stop asking for Keychain access. The gitignored `Signing.local.xcconfig` names the team and includes that file.

The recipe, which entitlements degrade under each signing and how to tell which mode a build runs in: [docs/signing.md](../../docs/signing.md).
