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
		.alert(
			"Folders now join a workspace inside its folder",
			isPresented: Binding(get: { !(app.setup?.workspaces.pendingConversion.isEmpty ?? true) }, set: { _ in }),
			presenting: app.setup?.workspaces,
		) { workspaces in
			Button("Link them") { workspaces.convertFolders(.link) }
				.keyboardShortcut(.defaultAction)
			Button("Move them") { workspaces.convertFolders(.move) }
		} message: { workspaces in
			Text(conversionMessage(workspaces.pendingConversion))
		}
	}

	/// The one-time question about an earlier build's further folders.
	private func conversionMessage(_ folders: [UnconvertedFolder]) -> String {
		let names = folders.map { "\(URL(filePath: $0.path).lastPathComponent) (\($0.workspaceName))" }.joined(separator: ", ")
		return "Vaultday keeps each workspace in one folder. These folders sit beside theirs: \(names). Link them to leave them where they are, or move them into their workspace's folder. Linked folders stay on this Mac; iCloud Drive does not carry links"
	}
}
