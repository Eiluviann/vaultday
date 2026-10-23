import Foundation
import Observation

/// One line of the Ready summary.
public struct ReadyLine: Identifiable, Hashable, Sendable {
	public let id: String
	public let icon: String
	public let title: String
	public let detail: String
}

/// The Ready step: what the setup chose, sphere by sphere, read back from the
/// app document and the workspaces, and the last line about where the notes live.
@MainActor
@Observable
public final class ReadyStepModel {
	public private(set) var lines: [WorkspaceSphere: [ReadyLine]] = [:]
	public private(set) var syncLine = ""

	private let session: CoreSession
	private let rhythm: RhythmModel

	public init(session: CoreSession, rhythm: RhythmModel) {
		self.session = session
		self.rhythm = rhythm
	}

	/// Reads the choices again for the given workspaces.
	public func refresh(workspaces: [WorkspaceEntry]) {
		rhythm.reload()
		var next: [WorkspaceSphere: [ReadyLine]] = [:]
		for sphere in WorkspaceSphere.allCases {
			let inSphere = workspaces.filter { $0.sphere == sphere }
			var sphereLines = inSphere.map { workspace in
				ReadyLine(
					id: "workspace-\(workspace.id)",
					icon: "folder",
					title: "Workspace · \(workspace.name)",
					detail: Self.folderCount(workspace.folders.count),
				)
			}
			let parts = Set(inSphere.flatMap { session.appState.accountParts(workspace: $0.id) }.map(AccountPartChoice.init))
			if !parts.isEmpty {
				let names = AccountPartChoice.allCases.filter { parts.contains($0) }.map(\.title)
				sphereLines.append(ReadyLine(id: "accounts-\(sphere.rawValue)", icon: "at", title: names.joined(separator: ", "), detail: "From your accounts, once they connect"))
			}
			if !inSphere.isEmpty {
				sphereLines.append(rhythmLine(for: sphere))
			}
			next[sphere] = sphereLines
		}
		lines = next
		syncLine = Self.syncText(for: session.appState.syncChoice())
	}

	private func rhythmLine(for sphere: WorkspaceSphere) -> ReadyLine {
		switch sphere {
		case .work:
			let hours = rhythm.workingHours
			let days = Self.dayRange(hours.days)
			return ReadyLine(
				id: "rhythm-work",
				icon: "clock",
				title: "\(days) \(hours.start.text) – \(hours.end.text)",
				detail: "planning \(rhythm.time(of: .planning).text) · shutdown \(rhythm.time(of: .shutdown).text)",
			)
		case .life:
			let review = rhythm.day(of: .projectReview)?.title ?? ""
			return ReadyLine(
				id: "rhythm-life",
				icon: "moon",
				title: "Evenings and weekends",
				detail: "light planning \(rhythm.time(of: .lightPlanning).text) · project review \(review) \(rhythm.time(of: .projectReview).text)",
			)
		}
	}

	static func folderCount(_ count: Int) -> String {
		count == 1 ? "1 folder" : "\(count) folders"
	}

	/// `Weekdays` for Monday to Friday, `Every day` for all seven, else the days' short names.
	static func dayRange(_ days: Set<WeekdayChoice>) -> String {
		let weekdays: Set<WeekdayChoice> = [.monday, .tuesday, .wednesday, .thursday, .friday]
		if days == weekdays { return "Weekdays" }
		if days.count == WeekdayChoice.allCases.count { return "Every day" }
		return WeekdayChoice.allCases.filter { days.contains($0) }.map(\.title).joined(separator: " ")
	}

	static func syncText(for choice: SyncChoiceView) -> String {
		switch choice.home {
		case .local:
			return "Your notes stay on this Mac, and only here"
		case .localAndCloud:
			return "Your notes stay on this Mac, mirrored to \(SyncServiceChoice(choice.service).title) for your other devices"
		case .cloud:
			return "Your notes live in \(SyncServiceChoice(choice.service).title), and your other devices read them from there"
		}
	}
}
