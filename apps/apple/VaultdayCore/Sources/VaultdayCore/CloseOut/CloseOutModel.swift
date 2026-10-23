import AppKit
import Foundation
import Observation

/// The week close-out ritual screen: four steps over one period, the current
/// step being the first with something left to do. Each action goes through
/// the core and reloads the snapshot, so what the screen shows is what the
/// workspace document holds.
@MainActor
@Observable
public final class CloseOutModel {
	public private(set) var snapshot: CloseOutSnapshot?
	/// Days adjusted and not confirmed yet, by `YYYY-MM-DD`.
	public private(set) var draft: [String: [CloseOutAllotment]] = [:]
	public private(set) var currentStep: CloseOutStepKind?
	public private(set) var errorMessage: String?
	/// The line the closing step wrote.
	public private(set) var closedLine: String?
	/// Whether the table was copied since it was shown.
	public private(set) var isTableCopied = false
	public var isPresented = false

	@ObservationIgnored private var source: (any CloseOutSource)?
	@ObservationIgnored private var kind: CloseOutPeriodKind = .week
	@ObservationIgnored private var date = ""
	@ObservationIgnored private let copy: @MainActor (String) -> Void

	/// `copy` puts the table on the pasteboard; tests pass a recorder.
	public init(copy: @escaping @MainActor (String) -> Void = CloseOutModel.copyToPasteboard) {
		self.copy = copy
	}

	/// Opens the close-out of the period of `kind` holding `date`.
	public func open(source: any CloseOutSource, kind: CloseOutPeriodKind, date: String) {
		self.source = source
		self.kind = kind
		self.date = date
		draft = [:]
		closedLine = nil
		isTableCopied = false
		reload()
		currentStep = firstOpenStep ?? steps.last?.kind
		isPresented = true
	}

	public var steps: [CloseOutStep] { snapshot?.steps ?? [] }

	/// "Week close-out · 19 – 23 Oct".
	public var title: String {
		guard let snapshot else { return "Close-out" }
		let noun = snapshot.kind == .week ? "Week close-out" : "Month close-out"
		return "\(noun) · \(CloseOutText.dayTitle(snapshot.start)) – \(CloseOutText.dayTitle(snapshot.end))"
	}

	/// The first step with something left to do.
	public var firstOpenStep: CloseOutStepKind? {
		steps.first { !$0.isSettled }?.kind
	}

	public var isFinished: Bool { snapshot?.isClosed ?? false }

	public func select(_ step: CloseOutStepKind) {
		guard steps.contains(where: { $0.kind == step }) else { return }
		currentStep = step
	}

	public var canGoBack: Bool { index(of: currentStep).map { $0 > 0 } ?? false }

	public var canGoNext: Bool { index(of: currentStep).map { $0 < steps.count - 1 } ?? false }

	public func goBack() {
		guard let index = index(of: currentStep), index > 0 else { return }
		currentStep = steps[index - 1].kind
	}

	public func goNext() {
		guard let index = index(of: currentStep), index < steps.count - 1 else { return }
		currentStep = steps[index + 1].kind
	}

	// MARK: Step 1

	/// Queues the Toggl entries through the feature set; nothing is sent from here.
	public func queueTogglEntries() {
		perform { source in
			_ = try source.queueToggl(kind: kind, date: date)
		}
	}

	// MARK: Step 2

	/// Sets `story` on `day` to `minutes`; the rest of the day re-balances to keep its total.
	public func setMinutes(_ minutes: Int, story: String, day: String) {
		guard let source else { return }
		do {
			draft = try source.rebalance(kind: kind, date: date, draft: draft, day: day, story: story, minutes: minutes)
			reloadSnapshot()
		} catch {
			errorMessage = coreErrorMessage(error)
		}
	}

	/// Confirms every day as shown, adjusted days included.
	public func confirmAllocation() {
		guard let snapshot else { return }
		let days = Dictionary(uniqueKeysWithValues: snapshot.allocation.map { day in
			(day.date, day.rows.map { CloseOutAllotment(story: $0.story, minutes: $0.minutes) })
		})
		perform { source in
			try source.confirmAllocation(kind: kind, date: date, days: days)
			draft = [:]
		}
	}

	/// Hourly and fixed price: the hours are reviewed.
	public func confirmReview() {
		perform { source in
			try source.confirmReview(kind: kind, date: date)
		}
	}

	// MARK: Step 3

	/// Puts the table's text on the pasteboard, as the sink takes it.
	public func copyTable() {
		guard let text = snapshot?.table.text else { return }
		copy(text)
		isTableCopied = true
	}

	/// Ticks or unticks a row as entered; kept in the workspace document.
	public func toggleTick(_ row: CloseOutTableRow) {
		guard let sink = snapshot?.table.sink else { return }
		perform { source in
			try source.setTicked(kind: kind, date: date, sink: sink, key: row.key, isTicked: !row.isTicked)
		}
	}

	// MARK: Step 4

	/// Writes the closing status line; `nowLocal` is for tests.
	public func close(nowLocal: String? = nil) {
		perform { source in
			closedLine = try source.close(kind: kind, date: date, nowLocal: nowLocal)
		}
	}

	public func reload() {
		reloadSnapshot()
	}

	public static func copyToPasteboard(_ text: String) {
		NSPasteboard.general.clearContents()
		NSPasteboard.general.setString(text, forType: .string)
	}

	// MARK: Private

	private func perform(_ action: (any CloseOutSource) throws -> Void) {
		guard let source else { return }
		let before = currentStep.flatMap { step in steps.first { $0.kind == step } }
		do {
			try action(source)
			errorMessage = nil
		} catch {
			errorMessage = coreErrorMessage(error)
		}
		reloadSnapshot()
		// The step just settled: move on to what is left.
		if let before, !before.isSettled, steps.first(where: { $0.kind == before.kind })?.isSettled == true {
			currentStep = firstOpenStep ?? steps.last?.kind
		}
	}

	private func reloadSnapshot() {
		guard let source else { return }
		do {
			snapshot = try source.closeOut(kind: kind, date: date, draft: draft)
		} catch {
			errorMessage = coreErrorMessage(error)
		}
	}

	private func index(of step: CloseOutStepKind?) -> Int? {
		guard let step else { return nil }
		return steps.firstIndex { $0.kind == step }
	}
}

/// The header's ritual button and the Monday alert for the open workspace.
@MainActor
@Observable
public final class CloseOutDueModel {
	/// The close-out due now, `nil` when none is.
	public private(set) var due: CloseOutDue?
	/// The Monday alert, while not dismissed.
	public private(set) var alert: CloseOutAlert?

	public init() {}

	/// Asks the source what is due at `nowLocal`.
	public func refresh(source: (any CloseOutSource)?, nowLocal: String? = nil) {
		due = (try? source?.due(nowLocal: nowLocal)) ?? nil
	}

	/// The Monday alert: the core decides, once per week; shown until dismissed.
	public func checkAlert(source: (any CloseOutSource)?, nowLocal: String? = nil) {
		guard let source, let fired = try? source.checkAlert(nowLocal: nowLocal) else { return }
		alert = fired
	}

	public func dismissAlert() {
		alert = nil
	}

	public func clear() {
		due = nil
		alert = nil
	}
}

/// The close-out keys of a workspace's `settings.json`, as the Configure
/// sheet edits them: the log deadline and the leave source. Nothing secret.
@MainActor
@Observable
public final class CloseOutSettingsModel {
	public let root: String
	/// The close-out runs for a workspace with a deadline.
	public var hasDeadline = false
	public var deadlineDay: WeekdayChoice = .monday
	/// `HH:MM`.
	public var deadlineTime = "08:00"
	/// The workspace whose `Leave.md` counts; `nil` for the life workspace.
	public var leaveSource: String?
	public private(set) var errorMessage: String?

	public init(root: String) {
		self.root = root
		reload()
	}

	public func reload() {
		do {
			let view = try readCloseOutSettings(root: root)
			hasDeadline = view.deadlineWeekday != nil
			deadlineDay = view.deadlineWeekday.map(WeekdayChoice.init) ?? .monday
			deadlineTime = view.deadlineTime
			leaveSource = view.leaveSource
			errorMessage = nil
		} catch {
			errorMessage = coreErrorMessage(error)
		}
	}

	@discardableResult
	public func save() -> Bool {
		let view = CloseOutSettingsView(
			deadlineWeekday: hasDeadline ? deadlineDay.core : nil,
			deadlineTime: deadlineTime.trimmingCharacters(in: .whitespaces),
			leaveSource: leaveSource,
		)
		do {
			_ = try updateCloseOutSettings(root: root, settings: view)
			reload()
			return true
		} catch {
			errorMessage = coreErrorMessage(error)
			return false
		}
	}
}
