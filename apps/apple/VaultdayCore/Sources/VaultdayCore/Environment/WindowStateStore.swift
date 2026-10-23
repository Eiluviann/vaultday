import Foundation

/// What the Home window remembers between launches.
public struct WindowState: Codable, Equatable, Sendable {
	public var isSidebarCollapsed: Bool
	public var focusedPane: Pane

	public init(isSidebarCollapsed: Bool = false, focusedPane: Pane = .today) {
		self.isSidebarCollapsed = isSidebarCollapsed
		self.focusedPane = focusedPane
	}
}

/// Window state in the App Group container's volatile folder, never in a
/// workspace. Losing the file only resets the window.
public struct WindowStateStore: Sendable {
	public let file: URL

	public init(layout: AppGroupLayout) {
		file = layout.windowStateFile
	}

	public func load() -> WindowState {
		guard let data = try? Data(contentsOf: file),
		      let state = try? JSONDecoder().decode(WindowState.self, from: data)
		else { return WindowState() }
		return state
	}

	public func save(_ state: WindowState) {
		guard let data = try? JSONEncoder().encode(state) else { return }
		try? FileManager.default.createDirectory(at: file.deletingLastPathComponent(), withIntermediateDirectories: true)
		try? data.write(to: file, options: [.atomic])
	}
}
