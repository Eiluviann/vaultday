// swift-tools-version: 6.0
// The Vaultday core for Swift: the UniFFI XCFramework (built by
// `scripts/build-xcframework.sh`, gitignored) plus the generated bindings and
// the `@Observable` models the app's views use. Views never see FFI types.
import PackageDescription

let package = Package(
	name: "VaultdayCore",
	platforms: [.macOS(.v15)],
	products: [
		.library(name: "VaultdayCore", targets: ["VaultdayCore"]),
	],
	targets: [
		.binaryTarget(name: "VaultdayCoreFFI", path: "VaultdayCoreFFI.xcframework"),
		.target(
			name: "VaultdayCore",
			dependencies: ["VaultdayCoreFFI"],
			path: "Sources/VaultdayCore",
		),
		.testTarget(
			name: "VaultdayCoreTests",
			dependencies: ["VaultdayCore"],
			path: "Tests/VaultdayCoreTests",
		),
	],
)
