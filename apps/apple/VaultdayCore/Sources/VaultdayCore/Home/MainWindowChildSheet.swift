import SwiftUI

extension EnvironmentValues {
	/// The app's one main window, set at its root; `nil` outside it.
	@Entry public var mainWindow: MainWindowModel?
}

public extension View {
	/// Marks a sheet's content as a child sheet of the main window, so ⌘,
	/// and Sync Check are ignored while it is open.
	func mainWindowChildSheet() -> some View {
		modifier(MainWindowChildSheet())
	}
}

/// Tells the main window when the sheet it is applied to opens and closes.
private struct MainWindowChildSheet: ViewModifier {
	@Environment(\.mainWindow) private var mainWindow
	@State private var id = UUID()

	func body(content: Content) -> some View {
		content
			.onAppear { mainWindow?.childSheetOpened(id) }
			.onDisappear { mainWindow?.childSheetClosed(id) }
	}
}
