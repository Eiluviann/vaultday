import AppKit
import VaultdayCore

/// The open panel the folder steps and Restore use, as a sheet of the main
/// window (or of the sheet over it) so it opens no window of its own. A
/// sandboxed app gets access to what the user picks here, which the core's
/// bookmarks then keep.
@MainActor
enum FolderPicker {
	/// Asks for one folder.
	///
	/// - Parameters:
	///   - message: The line above the panel's list.
	///   - prompt: The panel's confirm button.
	///   - canCreate: Whether the panel offers New Folder.
	///   - directory: Where the panel opens.
	static func pick(message: String, prompt: String, canCreate: Bool = true, directory: URL? = nil) async -> URL? {
		let panel = NSOpenPanel()
		panel.canChooseFiles = false
		panel.canChooseDirectories = true
		panel.allowsMultipleSelection = false
		panel.canCreateDirectories = canCreate
		panel.message = message
		panel.prompt = prompt
		panel.directoryURL = directory
		let response: NSApplication.ModalResponse
		if let window = sheetHost(in: NSApp.windows) {
			response = await panel.beginSheetModal(for: window)
		} else {
			response = await panel.begin()
		}
		return response == .OK ? panel.url : nil
	}

	/// The window the panel attaches to: the visible main window, or the
	/// topmost sheet over it; `nil` when the main window is closed. Never the
	/// key window as such: the Add and Restore popovers are still key while
	/// they close, and a panel attached to one would close with it.
	static func sheetHost(in windows: [NSWindow]) -> NSWindow? {
		let main = windows.first { window in
			window.isVisible && window.sheetParent == nil && MainWindowModel.isMainWindow(lineage: [window.identifier?.rawValue])
		}
		guard var host = main else { return nil }
		while let sheet = host.attachedSheet {
			host = sheet
		}
		return host
	}

	/// Where iCloud Drive keeps its folders on this Mac.
	static var iCloudDriveFolder: URL {
		FileManager.default.homeDirectoryForCurrentUser
			.appending(path: "Library/Mobile Documents/com~apple~CloudDocs", directoryHint: .isDirectory)
	}
}

extension String {
	/// A path as the steps show it: the home folder written `~`.
	var abbreviatedPath: String { (self as NSString).abbreviatingWithTildeInPath }
}
