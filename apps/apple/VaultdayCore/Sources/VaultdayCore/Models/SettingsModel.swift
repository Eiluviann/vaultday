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
	public private(set) var connections: [ConnectionSummary] = []
	/// Snapshots refused at open, for the user to see.
	public private(set) var refusedSnapshots: [String] = []
	public private(set) var errorMessage: String?

	private let session: CoreSession

	public init(session: CoreSession) {
		self.session = session
		reload()
	}

	/// Reads the settings again.
	public func reload() {
		let appState = session.appState
		keyboardScheme = appState.keyboardScheme() == .vim ? .vimMotions : .standard
		hoverSelects = appState.hoverSelects()
		connections = appState.connections().map(mapConnection)
		refusedSnapshots = session.refusedSnapshots
	}

	public func setKeyboardScheme(_ scheme: KeyboardSchemeSetting) {
		write { try $0.setKeyboardScheme(scheme: scheme == .vimMotions ? .vim : .standard) }
	}

	public func setHoverSelects(_ isOn: Bool) {
		write { try $0.setHoverSelects(isOn: isOn) }
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
