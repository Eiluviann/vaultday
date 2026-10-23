import SwiftUI
import VaultdayCore

/// The Today column: the spike's list of open work from the core, or the
/// folder grant when no workspace is open yet.
struct TodayColumn: View {
	let app: AppModel
	@State private var isPickingFolder = false

	var body: some View {
		Group {
			if let setupError = app.setupError {
				ContentUnavailableView("Vaultday could not start", systemImage: "exclamationmark.triangle", description: Text(setupError))
			} else if let workspace = app.workspace {
				switch workspace.phase {
				case .open:
					TodayList(plan: app.dayPlan, focus: app.focus)
				case .noFolder:
					grantPrompt(message: "Vaultday reads a workspace folder in place; nothing is copied or moved.")
				case let .failed(message):
					grantPrompt(message: message)
				}
			} else {
				ProgressView()
			}
		}
		.frame(maxWidth: .infinity, maxHeight: .infinity)
		.background(Tokens.Colors.surface)
		.paneFocus(app.focus.focusedPane == .today)
		.fileImporter(isPresented: $isPickingFolder, allowedContentTypes: [.folder]) { result in
			if case let .success(url) = result {
				app.grantFolder(url)
			}
		}
	}

	private func grantPrompt(message: String) -> some View {
		ContentUnavailableView {
			Label("Open a workspace folder", systemImage: "folder")
		} description: {
			Text(message)
		} actions: {
			Button("Choose folder") { isPickingFolder = true }
				.buttonStyle(.borderedProminent)
		}
	}
}

/// The day's open tasks by story, and the day log.
private struct TodayList: View {
	let plan: DayPlanModel
	let focus: PaneFocusModel

	var body: some View {
		let rows = taskRows
		List {
			if let message = plan.errorMessage {
				Text(message).foregroundStyle(Tokens.Colors.danger)
			}
			ForEach(plan.stories) { story in
				Section(storyHeading(story)) {
					ForEach(rows.filter { $0.storyId == story.id }) { row in
						TaskRow(task: row.task, isPendingInput: row.isPendingInput, isSelected: isSelected(row.index))
							.contentShape(Rectangle())
							.onTapGesture { focus.select(row.index, in: .today) }
					}
				}
			}
			Section("Log \(plan.date)") {
				if plan.log.isEmpty {
					Text("No log lines for this day").foregroundStyle(Tokens.Colors.inkMuted)
				}
				ForEach(plan.log) { line in
					HStack(alignment: .firstTextBaseline) {
						Text(line.time).monospacedDigit().foregroundStyle(Tokens.Colors.inkMuted)
						Text(line.text).foregroundStyle(Tokens.Colors.ink)
						Spacer()
						if let kind = line.kind {
							Text(kind).font(Tokens.Typography.caption.font).foregroundStyle(Tokens.Colors.inkMuted)
						}
					}
				}
			}
		}
		.scrollContentBackground(.hidden)
		.font(Tokens.Typography.body.font)
		.navigationTitle(plan.workspaceName.isEmpty ? "Today" : "\(plan.workspaceName) · Today")
		.task { LaunchTimer.markFirstBody(todayMilliseconds: plan.lastCallMilliseconds) }
	}

	private struct Row: Identifiable {
		let index: Int
		let storyId: String
		let task: TodayTask
		let isPendingInput: Bool
		var id: String { task.id }
	}

	/// Every task row in display order, numbered for the selection.
	private var taskRows: [Row] {
		let tasks = plan.stories.flatMap { story in
			story.nextSteps.map { (story.id, $0, false) } + story.pendingInput.map { (story.id, $0, true) }
		}
		return tasks.enumerated().map { index, entry in
			Row(index: index, storyId: entry.0, task: entry.1, isPendingInput: entry.2)
		}
	}

	private func isSelected(_ index: Int) -> Bool {
		focus.focusedPane == .today && focus.selection(in: .today) == index
	}

	private func storyHeading(_ story: TodayStory) -> String {
		[story.number, story.title].compactMap { $0 }.joined(separator: " · ")
	}
}

private struct TaskRow: View {
	let task: TodayTask
	let isPendingInput: Bool
	let isSelected: Bool

	var body: some View {
		HStack(alignment: .firstTextBaseline, spacing: Tokens.Spacing.space8) {
			Image(systemName: task.isInProgress ? "circle.lefthalf.filled" : (isPendingInput ? "person.crop.circle.badge.questionmark" : "circle"))
				.foregroundStyle(Tokens.Colors.inkMuted)
			Text(task.text).foregroundStyle(Tokens.Colors.ink)
			Spacer()
			if let assignee = task.assignedTo {
				Text(assignee).font(Tokens.Typography.callout.font).foregroundStyle(Tokens.Colors.inkMuted)
			}
			if let estimate = task.estimate {
				Text(estimate).font(Tokens.Typography.callout.font).monospacedDigit().foregroundStyle(Tokens.Colors.inkSecondary)
			}
		}
		.padding(.leading, CGFloat(task.depth) * 18)
		.listRowBackground(
			RoundedRectangle(cornerRadius: Tokens.Radius.radiusMd)
				.fill(isSelected ? Tokens.Colors.sunriseSoft : .clear)
		)
	}
}
