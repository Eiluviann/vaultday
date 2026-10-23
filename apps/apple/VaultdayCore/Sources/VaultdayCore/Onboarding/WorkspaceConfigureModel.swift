import Foundation
import Observation

/// The Configure sheet of one workspace: the fields of its
/// `.vaultday/settings.json` as text the controls edit. The core checks every
/// value and writes the file; keys this build does not know are kept. A
/// draft, the sheet of a workspace being added, writes nothing until its
/// first save, which hands the values to the draft's commit instead.
@MainActor
@Observable
public final class WorkspaceConfigureModel {
	public var name = ""
	public var sphere: WorkspaceSphere = .work
	/// A workspace color word, or empty for none.
	public var accent = ""
	public var ticketPattern = ""
	public var ticketLink = ""
	/// Hosts, one per line or separated by commas.
	public var hosts = ""
	public var browser = ""
	public var hoursPerDay = "8"
	public var logFolder = "Log"
	public var storyTemplate = ""

	/// One message per field the core refused, by `settings.json` key.
	public private(set) var issues: [String: String] = [:]
	/// Set when the file could not be read or written at all.
	public private(set) var errorMessage: String?
	public private(set) var isLoaded = false
	/// True until a draft's first save created the workspace.
	public private(set) var isDraft = false

	/// The workspace folder.
	public let root: String
	/// The `settings.json` path the sheet's footnote names.
	public var settingsPath: String { "\(root)/.vaultday/settings.json" }

	private var loaded: WorkspaceFormView?
	private var draftForm: WorkspaceFormView?
	private let commit: (@MainActor (WorkspaceFormView) throws -> Void)?

	public init(root: String) {
		self.root = root
		commit = nil
		reload()
	}

	/// The sheet of a workspace being added: it starts from `form`, and its
	/// first save calls `commit`, which creates the workspace; nothing is
	/// written before.
	public init(draft form: WorkspaceFormView, root: String, commit: @escaping @MainActor (WorkspaceFormView) throws -> Void) {
		self.root = root
		self.commit = commit
		draftForm = form
		isDraft = true
		reload()
	}

	/// Whether any field differs from the file.
	public var hasChanges: Bool {
		guard let loaded else { return false }
		return form() != loaded
	}

	/// Reads the file again, dropping edits; a draft goes back to its first values.
	public func reload() {
		if isDraft, let draftForm {
			apply(draftForm)
			loaded = draftForm
			issues = [:]
			errorMessage = nil
			isLoaded = true
			return
		}
		do {
			let form = try readWorkspaceForm(root: root)
			apply(form)
			loaded = form
			issues = [:]
			errorMessage = nil
			isLoaded = true
		} catch {
			errorMessage = coreErrorMessage(error)
			isLoaded = false
		}
	}

	/// Asks the core to check the fields now; fills ``issues``.
	@discardableResult
	public func validate() -> Bool {
		let found = workspaceFormIssues(form: form())
		issues = Dictionary(found.map { ($0.field, $0.message) }, uniquingKeysWith: { first, _ in first })
		return found.isEmpty
	}

	/// Writes the fields to `settings.json`.
	///
	/// - Returns: Whether the file was written; otherwise ``issues`` or ``errorMessage`` say why.
	@discardableResult
	public func save() -> Bool {
		guard validate() else { return false }
		if isDraft, let commit {
			do {
				let form = form()
				try commit(form)
				isDraft = false
				loaded = form
				errorMessage = nil
				return true
			} catch {
				errorMessage = coreErrorMessage(error)
				return false
			}
		}
		do {
			let written = try updateWorkspaceForm(root: root, form: form())
			loaded = written
			apply(written)
			errorMessage = nil
			return true
		} catch {
			errorMessage = coreErrorMessage(error)
			return false
		}
	}

	private func form() -> WorkspaceFormView {
		WorkspaceFormView(
			name: name.trimmingCharacters(in: .whitespacesAndNewlines),
			kind: sphere.core,
			accent: Self.optional(accent),
			ticketPattern: Self.optional(ticketPattern),
			ticketUrl: Self.optional(ticketLink),
			hosts: hosts.components(separatedBy: CharacterSet(charactersIn: ",\n")).compactMap { Self.optional($0) },
			browser: Self.optional(browser),
			// A value that is not a number goes to the core as NaN, which it refuses with its own message.
			hoursPerDay: Double(hoursPerDay.trimmingCharacters(in: .whitespaces)) ?? .nan,
			logFolder: logFolder.trimmingCharacters(in: .whitespaces),
			storyTemplate: Self.optional(storyTemplate),
		)
	}

	private func apply(_ form: WorkspaceFormView) {
		name = form.name
		sphere = WorkspaceSphere(form.kind)
		accent = form.accent ?? ""
		ticketPattern = form.ticketPattern ?? ""
		ticketLink = form.ticketUrl ?? ""
		hosts = form.hosts.joined(separator: "\n")
		browser = form.browser ?? ""
		hoursPerDay = Self.hoursText(form.hoursPerDay)
		logFolder = form.logFolder
		storyTemplate = form.storyTemplate ?? ""
	}

	private static func optional(_ text: String) -> String? {
		let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
		return trimmed.isEmpty ? nil : trimmed
	}

	private static func hoursText(_ hours: Double) -> String {
		hours.rounded() == hours ? String(Int(hours)) : String(hours)
	}
}
