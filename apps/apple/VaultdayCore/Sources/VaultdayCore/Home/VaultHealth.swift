import Foundation

/// What the sidebar footer's health dot summarises for the open workspace:
/// the vault as the core read it, and the app state's sync.
public struct VaultHealth: Equatable, Sendable {
	public let files: Int
	public let conflictCopies: [String]
	public let tasksWithoutId: Int
	public let unreadable: [String]
	/// Change files other devices wrote that have not arrived yet.
	public let pendingSyncFiles: Int
	/// Folder grants that no longer resolve.
	public let grantProblems: Int

	public init(files: Int, conflictCopies: [String], tasksWithoutId: Int, unreadable: [String], pendingSyncFiles: Int, grantProblems: Int) {
		self.files = files
		self.conflictCopies = conflictCopies
		self.tasksWithoutId = tasksWithoutId
		self.unreadable = unreadable
		self.pendingSyncFiles = pendingSyncFiles
		self.grantProblems = grantProblems
	}

	/// One line per thing that needs a look, in the order the list shows them.
	public var issues: [String] {
		var issues = conflictCopies.map { "Conflict copy · \($0)" }
		if tasksWithoutId > 0 {
			issues.append(tasksWithoutId == 1 ? "1 task line without an id" : "\(tasksWithoutId) task lines without an id")
		}
		issues += unreadable.map { "Could not read \($0)" }
		if pendingSyncFiles > 0 {
			issues.append(pendingSyncFiles == 1 ? "1 change file not arrived yet" : "\(pendingSyncFiles) change files not arrived yet")
		}
		if grantProblems > 0 {
			issues.append(grantProblems == 1 ? "1 folder needs granting again" : "\(grantProblems) folders need granting again")
		}
		return issues
	}

	public var isClean: Bool { issues.isEmpty }

	/// The footer's word beside the dot.
	public var label: String { isClean ? "Synced" : (issues.count == 1 ? "1 item" : "\(issues.count) items") }

	/// The dot's tooltip.
	public var summary: String {
		let files = "\(files) files"
		return isClean ? "Vault read · \(files) · no conflicts, no lines without id" : (["Vault health · \(files)"] + issues).joined(separator: "\n")
	}
}

extension VaultHealth {
	init(_ view: VaultHealthView, pendingSyncFiles: Int, grantProblems: Int) {
		self.init(
			files: Int(view.files),
			conflictCopies: view.conflictCopies,
			tasksWithoutId: Int(view.tasksWithoutId),
			unreadable: view.unreadable,
			pendingSyncFiles: pendingSyncFiles,
			grantProblems: grantProblems,
		)
	}
}
