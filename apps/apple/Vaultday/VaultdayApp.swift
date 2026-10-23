import SwiftUI
import VaultdayCore

@main
struct VaultdayApp: App {
	@State private var app = AppModel()
	@Environment(\.openWindow) private var openWindow

	var body: some Scene {
		Window("Vaultday", id: "home") {
			HomeWindow(app: app)
				.frame(minWidth: 1100, minHeight: 640)
				.task {
					app.launch()
					if app.needsSetup { openWindow(id: "setup") }
				}
				.onOpenURL { app.handle($0) }
		}
		.windowStyle(.hiddenTitleBar)
		.defaultSize(width: 1440, height: 900)
		.commands {
			CommandGroup(after: .appInfo) {
				Button("Set up Vaultday…") { openWindow(id: "setup") }
			}
		}

		Window("Set up Vaultday", id: "setup") {
			SetupWindow(app: app)
				.frame(minWidth: SetupLayout.sidebarWidth + SetupLayout.contentMinWidth, minHeight: 640)
		}
		.windowStyle(.hiddenTitleBar)
		.defaultSize(SetupLayout.windowDefaultSize)

		Settings {
			SettingsWindow(app: app)
		}

		Window("Sync Check", id: "sync-check") {
			SyncCheckWindow(app: app)
		}
		.windowResizability(.contentSize)

		// The timer from anywhere: the running entry, recent tasks, ⌘N.
		MenuBarExtra {
			MenuBarContent(app: app)
		} label: {
			MenuBarLabel(app: app)
		}
		.menuBarExtraStyle(.window)
	}
}
