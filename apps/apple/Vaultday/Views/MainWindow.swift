import SwiftUI
import VaultdayCore

/// The app's one window: the setup flow or Home at its root, and Settings or
/// the sync check as a sheet over either. Nothing opens a second window.
struct MainWindow: View {
	let app: AppModel

	var body: some View {
		Group {
			switch app.mainWindow.root {
			case .undecided:
				Color.clear
			case .setup:
				SetupScreen(app: app)
			case .home:
				HomeWindow(app: app)
			}
		}
		// Either root takes the whole window, so the setup steps never cap its size.
		.frame(maxWidth: .infinity, maxHeight: .infinity)
		.sheet(item: Binding(get: { app.mainWindow.panel }, set: { app.mainWindow.panel = $0 })) { panel in
			switch panel {
			case .settings:
				SettingsSheet(app: app)
			case .syncCheck:
				SyncCheckSheet(app: app)
			}
		}
		.environment(\.mainWindow, app.mainWindow)
	}
}
