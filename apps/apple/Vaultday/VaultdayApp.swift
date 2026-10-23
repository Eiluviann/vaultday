import SwiftUI
import VaultdayCore

/// One window: setup and Home fill it in turn, and Settings and the sync
/// check open in it as sheets. The menu-bar extra and the mini tracker bar
/// (⌥⌘T) are the only surfaces outside it.
@main
struct VaultdayApp: App {
	@State private var app = AppModel()
	@Environment(\.openWindow) private var openWindow

	var body: some Scene {
		Window("Vaultday", id: MainWindowModel.sceneId) {
			MainWindow(app: app)
				.frame(minWidth: 1100, minHeight: 640)
				.task {
					app.showMainWindowScene = { openWindow(id: MainWindowModel.sceneId) }
					app.launch()
				}
				.onOpenURL { app.handle($0) }
		}
		.windowStyle(.hiddenTitleBar)
		.defaultSize(width: 1440, height: 900)
		.commands {
			CommandGroup(after: .appInfo) {
				Button("Set up Vaultday…") { app.showSetup() }
			}
			CommandGroup(replacing: .appSettings) {
				// The key comes from Keymap.yaml's settings.toggle; the menu takes it
				// first, so ⌘, opens and closes Settings with a sheet key too.
				Button("Settings…") { app.toggleSettings() }
					.keyboardShortcut(settingsShortcut)
			}
			CommandGroup(after: .windowList) {
				Button("Sync Check") { app.showSyncCheck() }
			}
		}

		// The timer from anywhere: the running entry, recent tasks, ⌘N.
		MenuBarExtra {
			MenuBarContent(app: app)
		} label: {
			MenuBarLabel(app: app)
		}
		.menuBarExtraStyle(.window)
	}

	private var settingsShortcut: KeyboardShortcut? {
		app.dispatcher.keymap.singleStroke(for: .toggleSettings, scheme: app.dispatcher.scheme).flatMap { KeyboardShortcut($0) }
	}
}
