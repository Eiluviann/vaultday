import AppKit
import SwiftUI

/// The main window's title bar as the boards draw it: one 38 pt band, the
/// height of a compact unified toolbar, with the traffic lights centred in
/// it. The window keeps its hidden title bar (SwiftUI draws the band); the
/// lights' container is stretched to the band and the lights centred in it,
/// again whenever AppKit lays the title bar out anew (resize, key changes,
/// leaving full screen). A real toolbar would give the height too, but it
/// draws its own backdrop over the band.
struct WindowChrome: ViewModifier {
	@State private var placer = TrafficLightPlacer()

	func body(content: Content) -> some View {
		content.background(WindowReader { placer.attach(to: $0) })
	}

	/// The band's height on the boards and in the design note's header decision.
	static let titleBarHeight: CGFloat = 38
}

extension View {
	/// The 38 pt title band with the traffic lights centred in it.
	func windowChrome() -> some View {
		modifier(WindowChrome())
	}
}

/// Keeps a window's traffic lights centred in the 38 pt band.
@MainActor
final class TrafficLightPlacer {
	private weak var window: NSWindow?
	private var observers: [NSObjectProtocol] = []

	func attach(to window: NSWindow) {
		guard self.window !== window else {
			place()
			return
		}
		self.window = window
		observers.forEach(NotificationCenter.default.removeObserver)
		let names: [Notification.Name] = [
			NSWindow.didResizeNotification, NSWindow.didEndLiveResizeNotification,
			NSWindow.didBecomeKeyNotification, NSWindow.didResignKeyNotification,
			NSWindow.didExitFullScreenNotification,
		]
		observers = names.map { name in
			NotificationCenter.default.addObserver(forName: name, object: window, queue: .main) { [weak self] _ in
				MainActor.assumeIsolated { self?.place() }
			}
		}
		place()
	}

	/// Stretches the lights' container to the band and centres each light in
	/// it; in full screen the system's own title bar is left alone.
	func place() {
		guard let window, !window.styleMask.contains(.fullScreen),
		      let close = window.standardWindowButton(.closeButton),
		      let container = close.superview?.superview
		else { return }
		let height = WindowChrome.titleBarHeight
		var frame = container.frame
		frame.size.height = height
		frame.origin.y = window.frame.height - height
		if container.frame != frame { container.frame = frame }
		for kind in [NSWindow.ButtonType.closeButton, .miniaturizeButton, .zoomButton] {
			guard let button = window.standardWindowButton(kind), let titlebar = button.superview else { continue }
			titlebar.frame = container.bounds
			var origin = button.frame.origin
			origin.y = TrafficLightPlacer.centredOrigin(buttonHeight: button.frame.height, bandHeight: height)
			if button.frame.origin != origin { button.setFrameOrigin(origin) }
		}
	}

	/// The light's bottom edge in the band's flipped-up coordinates.
	nonisolated static func centredOrigin(buttonHeight: CGFloat, bandHeight: CGFloat) -> CGFloat {
		((bandHeight - buttonHeight) / 2).rounded()
	}
}
