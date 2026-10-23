import AppKit
import SwiftUI
import VaultdayCore

/// The mini tracker's always-on-top floating panel: on every Space, over
/// full-screen apps, draggable anywhere on the bar, snapped to the nearest
/// screen edge when a drag ends, hidden while the main window is frontmost.
/// It follows the app model: it shows, hides and resizes itself.
@MainActor
final class MiniTrackerController: NSObject, NSWindowDelegate {
	private var panel: NSPanel?
	private weak var app: AppModel?
	/// Set while the controller moves the panel itself, so the move is not snapped again.
	private var isPlacing = false

	func attach(app: AppModel) {
		self.app = app
		follow()
	}

	/// Re-applies the model whenever what the panel shows changes.
	private func follow() {
		guard let app else { return }
		withObservationTracking {
			apply(isVisible: app.isMiniTrackerVisible, isExpanded: app.isMiniTrackerExpanded)
		} onChange: { [weak self] in
			Task { @MainActor in self?.follow() }
		}
	}

	private func apply(isVisible: Bool, isExpanded: Bool) {
		guard isVisible, let app else {
			panel?.orderOut(nil)
			return
		}
		let panel = panel ?? makePanel(app: app)
		resize(panel, to: isExpanded ? MiniTrackerLayout.expandedSize : MiniTrackerLayout.collapsedSize)
		panel.orderFrontRegardless()
	}

	private func makePanel(app: AppModel) -> NSPanel {
		let size = MiniTrackerLayout.collapsedSize
		let panel = NSPanel(
			contentRect: NSRect(origin: .zero, size: size),
			styleMask: [.borderless, .nonactivatingPanel, .fullSizeContentView],
			backing: .buffered,
			defer: true,
		)
		panel.level = .floating
		panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .stationary]
		panel.isFloatingPanel = true
		panel.hidesOnDeactivate = false
		panel.becomesKeyOnlyIfNeeded = true
		panel.isMovableByWindowBackground = true
		panel.isOpaque = false
		panel.backgroundColor = .clear
		panel.hasShadow = true
		panel.title = "Mini tracker"
		panel.delegate = self
		panel.contentView = NSHostingView(rootView: MiniTrackerView(app: app))
		if let visible = NSScreen.main?.visibleFrame {
			// First shown at the bottom right, on the edge it snaps to.
			let origin = CGPoint(x: visible.maxX - size.width - MiniTrackerLayout.edgeMargin, y: visible.minY + MiniTrackerLayout.edgeMargin)
			place(panel, at: origin)
		}
		self.panel = panel
		return panel
	}

	/// Resizes and keeps the top edge where it was, so the bar unfolds downward.
	private func resize(_ panel: NSPanel, to size: CGSize) {
		guard panel.frame.size != size else { return }
		let top = panel.frame.maxY
		isPlacing = true
		panel.setFrame(NSRect(x: panel.frame.minX, y: top - size.height, width: size.width, height: size.height), display: true)
		isPlacing = false
		snap(panel)
	}

	func windowDidMove(_ notification: Notification) {
		guard !isPlacing, let panel = notification.object as? NSPanel else { return }
		snap(panel)
	}

	private func snap(_ panel: NSPanel) {
		guard let visible = (panel.screen ?? NSScreen.main)?.visibleFrame else { return }
		let origin = MiniTrackerLayout.snappedOrigin(for: panel.frame, in: visible)
		guard origin != panel.frame.origin else { return }
		place(panel, at: origin)
	}

	private func place(_ panel: NSPanel, at origin: CGPoint) {
		isPlacing = true
		panel.setFrameOrigin(origin)
		isPlacing = false
	}
}
