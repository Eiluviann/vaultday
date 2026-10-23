import AppKit
import Foundation
import Observation

/// The keyboard scheme, as the views see it.
public enum KeyboardSchemeSetting: String, CaseIterable, Hashable, Sendable {
	case standard
	case vimMotions
}

/// A connection of the app-level document, as the views see it.
public struct ConnectionSummary: Identifiable, Hashable, Sendable {
	public enum Kind: Hashable, Sendable {
		case vault, folder, calendar, reminderList, mailAccount
	}

	public let id: String
	public let kind: Kind
	public let label: String
}

/// App-level settings from the signed app document. Every change is
/// written as this device's snapshot at once.
@MainActor
@Observable
public final class SettingsModel {
	public private(set) var keyboardScheme: KeyboardSchemeSetting = .standard
	public private(set) var hoverSelects = true
	public private(set) var shortcutCaps: ShortcutCapsSetting = .animated
	/// Screen-share mode on this Mac, and the selection to come back to when it ends; neither syncs.
	public private(set) var isScreenShareOn = false
	public private(set) var screenShareReturnTo: String?
	/// This device's id, read-only: the sync folder names this Mac's change files by it.
	public let deviceId: String
	/// A line about the id for the device block, saying what it is for.
	public static let deviceIdNote = "Names this Mac's change files in each workspace's sync folder; it is new after a restore"
	public private(set) var connections: [ConnectionSummary] = []
	/// Snapshots refused at open, for the user to see.
	public private(set) var refusedSnapshots: [String] = []
	public private(set) var errorMessage: String?

	private let session: CoreSession
	private let copyToPasteboard: @MainActor (String) -> Void

	public init(
		session: CoreSession,
		copyToPasteboard: @escaping @MainActor (String) -> Void = { text in
			NSPasteboard.general.clearContents()
			NSPasteboard.general.setString(text, forType: .string)
		},
	) {
		self.session = session
		self.copyToPasteboard = copyToPasteboard
		deviceId = session.deviceId
		reload()
	}

	/// Reads the settings again.
	public func reload() {
		let appState = session.appState
		keyboardScheme = appState.keyboardScheme() == .vim ? .vimMotions : .standard
		hoverSelects = appState.hoverSelects()
		shortcutCaps = ShortcutCapsSetting(appState.shortcutCaps())
		isScreenShareOn = appState.screenShareMode()
		screenShareReturnTo = appState.screenShareReturnTo()
		connections = appState.connections().map(mapConnection)
		refusedSnapshots = session.refusedSnapshots
	}

	public func setKeyboardScheme(_ scheme: KeyboardSchemeSetting) {
		write { try $0.setKeyboardScheme(scheme: scheme == .vimMotions ? .vim : .standard) }
	}

	public func setHoverSelects(_ isOn: Bool) {
		write { try $0.setHoverSelects(isOn: isOn) }
	}

	public func setShortcutCaps(_ caps: ShortcutCapsSetting) {
		write { try $0.setShortcutCaps(caps: caps.core) }
	}

	/// Writes screen-share mode and the selection to restore, for this Mac only.
	public func setScreenShare(isOn: Bool, returnTo: String?) {
		write {
			try $0.setScreenShareMode(isOn: isOn)
			try $0.setScreenShareReturnTo(selection: returnTo)
		}
	}

	/// Copies the device id for pasting into a support note or a bug report.
	public func copyDeviceId() {
		copyToPasteboard(deviceId)
	}

	private func write(_ edit: (AppStateHandle) throws -> Void) {
		do {
			try edit(session.appState)
			try session.save()
			errorMessage = nil
		} catch {
			errorMessage = coreErrorMessage(error)
		}
		reload()
	}
}

private func mapConnection(_ connection: ConnectionView) -> ConnectionSummary {
	let kind: ConnectionSummary.Kind = switch connection.kind {
	case .vault: .vault
	case .folder: .folder
	case .calendar: .calendar
	case .reminderList: .reminderList
	case .mailAccount: .mailAccount
	}
	return ConnectionSummary(id: connection.id, kind: kind, label: connection.label)
}
