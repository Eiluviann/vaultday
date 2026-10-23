import Foundation
import Observation

/// A part of an account a workspace wants Vaultday to bring in.
public enum AccountPartChoice: String, CaseIterable, Identifiable, Hashable, Sendable {
	case mail
	case calendar
	case contacts
	case reminders

	public var id: String { rawValue }

	public var title: String {
		switch self {
		case .mail: "Mail"
		case .calendar: "Calendar"
		case .contacts: "Contacts"
		case .reminders: "Reminders"
		}
	}

	var core: AccountPartView {
		switch self {
		case .mail: .mail
		case .calendar: .calendar
		case .contacts: .contacts
		case .reminders: .reminders
		}
	}

	init(_ view: AccountPartView) {
		switch view {
		case .mail: self = .mail
		case .calendar: self = .calendar
		case .contacts: self = .contacts
		case .reminders: self = .reminders
		}
	}
}

/// The Accounts step. Mail accounts connect in a later release, so the Add
/// key is disabled and tagged Planned; what each workspace wants from its
/// accounts is stored in the app-level document for then.
@MainActor
@Observable
public final class AccountsStepModel {
	/// Whether an account can be added in this release.
	public let canAddAccounts = false
	/// The line that says why not.
	public static let plannedNote = "Mail accounts connect in a later release; the parts you tick are remembered for then"

	public private(set) var parts: [String: Set<AccountPartChoice>] = [:]
	public private(set) var errorMessage: String?

	private let session: CoreSession

	public init(session: CoreSession) {
		self.session = session
	}

	/// Reads what each of the given workspaces wants.
	public func refresh(workspaces: [WorkspaceEntry]) {
		parts = Dictionary(uniqueKeysWithValues: workspaces.map { workspace in
			(workspace.id, Set(session.appState.accountParts(workspace: workspace.id).map(AccountPartChoice.init)))
		})
	}

	public func isOn(_ part: AccountPartChoice, workspace: String) -> Bool {
		parts[workspace, default: []].contains(part)
	}

	public func set(_ part: AccountPartChoice, isOn: Bool, workspace: String) {
		var chosen = parts[workspace, default: []]
		if isOn { chosen.insert(part) } else { chosen.remove(part) }
		do {
			let ordered = AccountPartChoice.allCases.filter { chosen.contains($0) }.map(\.core)
			try session.edit { try $0.setAccountParts(workspace: workspace, parts: ordered) }
			parts[workspace] = chosen
			errorMessage = nil
		} catch {
			errorMessage = coreErrorMessage(error)
		}
	}
}
