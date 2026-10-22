import SwiftUI
import VaultdayCore

/// The spike's one window: the fixture workspace's open work and day log,
/// loaded through `TodayModel`. Reads only plain model values.
struct TodayWindow: View {
	let model: TodayModel

	var body: some View {
		Group {
			if let message = model.errorMessage {
				ContentUnavailableView("Could not open the workspace", systemImage: "exclamationmark.triangle", description: Text(message))
			} else {
				List {
					ForEach(model.stories) { story in
						Section(storyHeading(story)) {
							ForEach(story.nextSteps) { task in
								TaskRow(task: task)
							}
							ForEach(story.pendingInput) { task in
								TaskRow(task: task, isPendingInput: true)
							}
						}
					}
					Section("Log \(model.date)") {
						if model.log.isEmpty {
							Text("No log lines for this day").foregroundStyle(.secondary)
						}
						ForEach(model.log) { line in
							HStack(alignment: .firstTextBaseline) {
								Text(line.time).monospacedDigit().foregroundStyle(.secondary)
								Text(line.text)
								Spacer()
								if let kind = line.kind {
									Text(kind).font(.caption).foregroundStyle(.secondary)
								}
							}
						}
					}
				}
			}
		}
		.navigationTitle(model.workspaceName.isEmpty ? "Today" : "\(model.workspaceName) · Today")
		.navigationSubtitle("\(model.openTaskCount) open tasks")
		.task {
			model.load(workspacePath: LaunchConfiguration.workspacePath)
			LaunchTimer.markFirstBody(todayMilliseconds: model.lastCallMilliseconds)
		}
	}

	private func storyHeading(_ story: TodayStory) -> String {
		[story.number, story.title].compactMap { $0 }.joined(separator: " · ")
	}
}

private struct TaskRow: View {
	let task: TodayTask
	var isPendingInput = false

	var body: some View {
		HStack(alignment: .firstTextBaseline, spacing: 8) {
			Image(systemName: task.isInProgress ? "circle.lefthalf.filled" : (isPendingInput ? "person.crop.circle.badge.questionmark" : "circle"))
				.foregroundStyle(.secondary)
			Text(task.text)
			Spacer()
			if let assignee = task.assignedTo {
				Text(assignee).font(.caption).foregroundStyle(.secondary)
			}
			if let estimate = task.estimate {
				Text(estimate).font(.caption).monospacedDigit().foregroundStyle(.secondary)
			}
		}
		.padding(.leading, CGFloat(task.depth) * 18)
	}
}
