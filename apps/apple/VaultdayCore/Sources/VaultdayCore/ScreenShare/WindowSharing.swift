import AppKit

/// A window whose capture the mode controls: `NSWindow` in the app, a fake in tests.
@MainActor
public protocol SharableWindow: AnyObject {
	var sharingType: NSWindow.SharingType { get set }
}

extension NSWindow: SharableWindow {}

/// The windows that must stay out of a screen share while the mode is on:
/// the mini tracker bar and the menu-bar extra's window. Each sets
/// `sharingType` to `.none` while the mode is on and `.readOnly` otherwise.
/// The main window is never registered: it stays shareable, which is the
/// point of the mode.
@MainActor
public final class WindowSharingRegistry {
	private struct Entry {
		weak var window: (any SharableWindow)?
	}

	private var entries: [Entry] = []
	/// Whether the mode is on now; set by whoever owns the mode.
	public var isScreenShareOn: () -> Bool = { false }

	public init() {}

	/// The sharing type for the mode: `.none` while on, `.readOnly` (the system default) otherwise.
	public static func sharingType(isScreenShareOn: Bool) -> NSWindow.SharingType {
		isScreenShareOn ? .none : .readOnly
	}

	/// Adds `window` once and sets its sharing type for the mode now.
	public func register(_ window: any SharableWindow) {
		entries.removeAll { $0.window == nil }
		if !entries.contains(where: { $0.window === window }) {
			entries.append(Entry(window: window))
		}
		window.sharingType = Self.sharingType(isScreenShareOn: isScreenShareOn())
	}

	/// Sets every registered window's sharing type for the mode now; call it when the mode changes.
	public func apply() {
		let type = Self.sharingType(isScreenShareOn: isScreenShareOn())
		for window in entries.compactMap(\.window) {
			window.sharingType = type
		}
	}

	/// How many registered windows are still alive.
	public var windowCount: Int {
		entries.compactMap(\.window).count
	}
}
