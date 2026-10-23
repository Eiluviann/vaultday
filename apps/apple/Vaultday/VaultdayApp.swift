import SwiftUI
import VaultdayCore

@main
struct VaultdayApp: App {
	@State private var app = AppModel()

	var body: some Scene {
		Window("Vaultday", id: "home") {
			HomeWindow(app: app)
				.frame(minWidth: 1100, minHeight: 640)
				.task { app.launch() }
				.onOpenURL { app.handle($0) }
		}
		.windowStyle(.hiddenTitleBar)
		.defaultSize(width: 1440, height: 900)

		Window("Sync Check", id: "sync-check") {
			SyncCheckWindow(app: app)
		}
		.windowResizability(.contentSize)
	}
}
