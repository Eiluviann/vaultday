import AppKit

/// The open panel the folder steps and Restore use. A sandboxed app gets
/// access to what the user picks here, which the core's bookmarks then keep.
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
		let response = await panel.begin()
		return response == .OK ? panel.url : nil
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
