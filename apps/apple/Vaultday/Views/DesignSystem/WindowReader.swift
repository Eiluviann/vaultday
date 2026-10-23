import AppKit
import SwiftUI

/// Hands the window a view sits in to `onWindow`, once it has one and again
/// whenever the view updates (a menu-bar popover keeps its window).
struct WindowReader: NSViewRepresentable {
	let onWindow: @MainActor (NSWindow) -> Void

	func makeNSView(context: Context) -> NSView {
		WindowReportingView()
	}

	func updateNSView(_ view: NSView, context: Context) {
		guard let view = view as? WindowReportingView else { return }
		view.onWindow = onWindow
		if let window = view.window { onWindow(window) }
	}
}

private final class WindowReportingView: NSView {
	var onWindow: (@MainActor (NSWindow) -> Void)?

	override func viewDidMoveToWindow() {
		super.viewDidMoveToWindow()
		if let window, let onWindow { MainActor.assumeIsolated { onWindow(window) } }
	}
}
