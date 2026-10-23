import Foundation

/// Where reports come from: the core in the app, a fake in tests. A source
/// builds a bundle without writing anything; `storeReport` is the one call
/// that writes, and it writes only the report file.
@MainActor
public protocol ReportSource: AnyObject {
	/// The clients a report can be asked for: the workspace, then each project.
	func clients() throws -> [String]
	/// The bundle for a story number or a client over `range`. Writes nothing.
	func bundle(subject: String, range: ReportRange) throws -> ReportBundle
	/// Writes the bundle's JSON to `_agent/reports/<from>-<to>.json` beside the subject.
	func storeReport(subject: String, range: ReportRange) throws -> WrittenReport
}

/// The core behind the Reports tab.
@MainActor
final class CoreReportSource: ReportSource {
	private let vault: WorkspaceHandle

	init(vault: WorkspaceHandle) {
		self.vault = vault
	}

	func clients() throws -> [String] {
		try VaultdayCore.reportClients(workspace: vault)
	}

	func bundle(subject: String, range: ReportRange) throws -> ReportBundle {
		let view = try VaultdayCore.reportBundle(workspace: vault, subject: subject, from: range.from, to: range.to)
		return ReportBundle(view)
	}

	func storeReport(subject: String, range: ReportRange) throws -> WrittenReport {
		let view = try VaultdayCore.writeReport(workspace: vault, subject: subject, from: range.from, to: range.to)
		return WrittenReport(relativePath: view.relativePath, absolutePath: view.absolutePath)
	}
}

extension ReportBundle {
	init(_ view: ReportBundleView) {
		self.init(
			subject: view.subject,
			isClient: view.subjectKind == "client",
			range: ReportRange(from: view.from, to: view.to),
			billing: view.billing.flatMap(ReportBilling.init(rawValue:)),
			stories: view.stories.map(ReportStory.init),
			logGroups: view.logGroups.map(ReportLogGroup.init),
			sessionCount: Int(view.sessionCount),
			unmeasuredSessionCount: Int(view.unmeasuredSessionCount),
			hours: ReportHours(view.hours),
			json: view.json,
			markdown: view.markdown,
		)
	}
}

extension ReportStory {
	init(_ view: ReportStoryView) {
		self.init(
			number: view.number,
			title: view.title,
			status: view.status,
			relativePath: view.relativePath,
			nextSteps: ReportTaskCounts(view.nextSteps),
			pendingInput: ReportTaskCounts(view.pendingInput),
		)
	}
}

extension ReportTaskCounts {
	init(_ view: ReportTaskCountsView) {
		self.init(total: Int(view.total), open: Int(view.open), inProgress: Int(view.inProgress), done: Int(view.done))
	}
}

extension ReportLogGroup {
	init(_ view: ReportLogGroupView) {
		self.init(kind: view.kind, lines: view.lines.map(ReportLogLine.init))
	}
}

extension ReportLogLine {
	init(_ view: ReportLogLineView) {
		self.init(
			date: view.date,
			time: view.time,
			text: view.text,
			references: [view.story, view.task, view.project].compactMap(\.self),
			agent: view.agent,
		)
	}
}

extension ReportStoryMinutes {
	init(_ view: ReportStoryMinutesView) {
		self.init(story: view.story, minutes: Int(view.minutes))
	}
}

extension ReportHours {
	init(_ view: ReportHoursView) {
		let byStory = view.byStory.map(ReportStoryMinutes.init)
		switch view.model {
		case "fte":
			self = .fte(minutes: Int(view.loggedMinutes), byStory: byStory)
		case "hourly":
			self = .hourly(
				roundingMinutes: Int(view.roundingMinutes ?? 0),
				billableMinutes: Int(view.billableMinutes),
				billable: byStory,
				nonBillableMinutes: Int(view.nonBillableMinutes),
				nonBillable: view.nonBillableByStory.map(ReportStoryMinutes.init),
			)
		case "fixed":
			self = .fixed(
				quotedMinutes: Int(view.quotedMinutes),
				deliveredMinutes: Int(view.deliveredMinutes),
				overrunMinutes: Int(view.overrunMinutes),
				tasks: view.tasks.map { task in
					ReportFixedTask(
						story: task.story,
						task: task.task,
						text: task.text,
						quotedMinutes: task.quotedMinutes.map(Int.init),
						deliveredMinutes: Int(task.deliveredMinutes),
						overrunMinutes: Int(task.overrunMinutes),
					)
				},
			)
		default:
			self = .logged(minutes: Int(view.loggedMinutes), byStory: byStory)
		}
	}
}
