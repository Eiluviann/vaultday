import AppKit
import Foundation
import Observation

/// The Reports tab: a subject, a date range with presets, the bundle the core
/// assembled, the two copy actions and Open in Claude Code. Reports stay
/// model-free here: the tab builds the bundle and hands it to Claude Code, and
/// nothing in the app calls a model. The only write is the report file, and
/// only when Open in Claude Code asks for it.
@MainActor
@Observable
public final class ReportsModel {
	/// What the report is about: a story number, or a client.
	public private(set) var subject: String?
	/// The clients a workspace-wide Reports tab can pick from.
	public private(set) var clients: [String] = []
	public private(set) var preset: ReportPreset = .thisWeek
	public private(set) var range: ReportRange?
	public private(set) var bundle: ReportBundle?
	public private(set) var errorMessage: String?
	/// The file Open in Claude Code just wrote, shown with a copyable path.
	public private(set) var writtenReport: WrittenReport?
	/// What the last copy put on the pasteboard, as a short confirmation.
	public private(set) var copyNotice: String?
	/// Screen-share mode: the tab offers only the FTE-safe variant, logged hours without billing detail.
	public var isScreenShare = false {
		didSet { if isScreenShare != oldValue { reload() } }
	}

	/// The custom range's days while Custom is picked.
	public var customFrom: Date
	public var customTo: Date

	private var source: (any ReportSource)?
	private let now: () -> Date
	private let calendar: Calendar
	private let copyText: (String) -> Void

	public init(
		now: @escaping () -> Date = Date.init,
		calendar: Calendar = .current,
		copyText: @escaping (String) -> Void = { text in
			NSPasteboard.general.clearContents()
			NSPasteboard.general.setString(text, forType: .string)
		},
	) {
		self.now = now
		self.calendar = calendar
		self.copyText = copyText
		customFrom = now()
		customTo = now()
		range = ReportRange.range(for: .thisWeek, now: now(), calendar: calendar)
	}

	// MARK: Source

	/// Points the model at the open workspace; the report shown goes.
	public func attach(to workspace: WorkspaceModel) {
		attach(source: workspace.vault.map(CoreReportSource.init))
	}

	/// Replaces where reports come from, for tests.
	public func attach(source: (any ReportSource)?) {
		self.source = source
		clients = (try? source?.clients()) ?? []
		clear()
	}

	/// Drops the report shown and the file noted; the subject and range stay.
	public func clear() {
		bundle = nil
		errorMessage = nil
		writtenReport = nil
		copyNotice = nil
	}

	// MARK: Variant

	/// The variants the tab offers now: the workspace's own hours, or only the FTE-safe ones while screen-share mode is on.
	public var offeredVariants: [ReportVariant] {
		isScreenShare ? [.fteSafe] : [.full]
	}

	/// The variant the tab builds and copies.
	public var variant: ReportVariant {
		offeredVariants.first ?? .full
	}

	/// What the tab says under the range while the mode narrows the report.
	public var variantNote: String? {
		isScreenShare ? "Logged hours only while screen-share mode is on" : nil
	}

	// MARK: Subject and range

	/// Reports on `subject` (a story number, or a client) from now on; reads it
	/// when `load` is set.
	public func show(subject: String?, load: Bool = false) {
		guard subject != self.subject else {
			if load { reload() }
			return
		}
		self.subject = subject
		clear()
		if load { reload() }
	}

	/// The clients, read again from the workspace.
	public func refreshClients() {
		clients = (try? source?.clients()) ?? []
		if subject == nil || subject.map({ !clients.contains($0) }) == true, let first = clients.first {
			show(subject: first)
		}
	}

	/// Picks a preset: a preset sets the range and reads the report; Custom
	/// keeps the days picked and waits for them.
	public func select(preset: ReportPreset) {
		self.preset = preset
		if preset == .custom {
			applyCustomRange()
		} else {
			range = ReportRange.range(for: preset, now: now(), calendar: calendar)
			reload()
		}
	}

	/// Sets the custom range's days and reads the report.
	public func setCustomRange(from: Date, to: Date) {
		customFrom = from
		customTo = to
		preset = .custom
		applyCustomRange()
	}

	/// The range as people read it: "2026-10-19 to 2026-10-25".
	public var rangeText: String {
		range.map { $0.from == $0.to ? $0.from : "\($0.from) to \($0.to)" } ?? ""
	}

	/// Reads the report for the subject and range. A reversed custom range is
	/// said, not sent to the core.
	public func reload() {
		writtenReport = nil
		copyNotice = nil
		guard let source, let subject else {
			bundle = nil
			return
		}
		guard let range, range.isValid else {
			bundle = nil
			errorMessage = "The range ends before it starts"
			return
		}
		do {
			bundle = try source.bundle(subject: subject, range: range, variant: variant)
			errorMessage = nil
		} catch {
			bundle = nil
			errorMessage = coreErrorMessage(error)
		}
	}

	/// Reads the report once, when its tab first shows.
	public func loadIfNeeded() {
		if bundle == nil, errorMessage == nil { reload() }
	}

	// MARK: Actions

	/// The link behind Open in Claude Code for the subject: the story's, or the client's.
	public var claudeCodeLink: URL? {
		guard let subject else { return nil }
		return VaultdayLink.url(host: "claude-code", path: (bundle?.isClient == true ? "/client/" : "/story/") + subject)
	}

	/// What Copy as JSON puts on the pasteboard.
	public var jsonPayload: String? { bundle?.json }

	/// What Copy as markdown puts on the pasteboard.
	public var markdownPayload: String? { bundle?.markdown }

	public func copyJSON() {
		guard let json = jsonPayload else { return }
		copyText(json)
		copyNotice = "Copied as JSON"
	}

	public func copyMarkdown() {
		guard let markdown = markdownPayload else { return }
		copyText(markdown)
		copyNotice = "Copied as markdown"
	}

	/// Copies the written file's path.
	public func copyWrittenPath() {
		guard let writtenReport else { return }
		copyText(writtenReport.absolutePath)
		copyNotice = "Copied the path"
	}

	/// Open in Claude Code, step one: writes the bundle to
	/// `_agent/reports/<from>-<to>.json` beside the subject and keeps the path
	/// to show. Returns the file, or `nil` when nothing was written. The
	/// caller then opens the folder through the Open in Claude Code action.
	@discardableResult
	public func writeForClaudeCode() -> WrittenReport? {
		guard let source, let subject, let range, range.isValid, bundle != nil else { return nil }
		do {
			let written = try source.storeReport(subject: subject, range: range, variant: variant)
			writtenReport = written
			errorMessage = nil
			return written
		} catch {
			writtenReport = nil
			errorMessage = coreErrorMessage(error)
			return nil
		}
	}

	private func applyCustomRange() {
		range = ReportRange(from: DayText.string(customFrom), to: DayText.string(customTo))
		reload()
	}
}
