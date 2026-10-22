import Foundation

/// Where the spike reads its workspace from: `VAULTDAY_WORKSPACE` in the
/// environment, else the fixture path XcodeGen bakes into Info.plist.
enum LaunchConfiguration {
	static var workspacePath: String {
		if let override = ProcessInfo.processInfo.environment["VAULTDAY_WORKSPACE"], !override.isEmpty {
			return override
		}
		let baked = Bundle.main.object(forInfoDictionaryKey: "VaultdayFixtureWorkspace")
		return (baked as? String) ?? ""
	}

	/// `VAULTDAY_QUIT_AFTER_PAINT=1` ends the process once the first body ran,
	/// so a shell can measure cold start without closing the window by hand.
	static var shouldQuitAfterPaint: Bool {
		ProcessInfo.processInfo.environment["VAULTDAY_QUIT_AFTER_PAINT"] == "1"
	}
}
