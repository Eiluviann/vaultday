import SwiftUI
import VaultdayCore

/// The mini tracker bar: collapsed, the workspace dot, the task, its story
/// and task ids, the elapsed time, pause and stop; expanded, the subtasks,
/// progress against the estimate, the next task and the actions.
struct MiniTrackerView: View {
	let app: AppModel

	var body: some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space8) {
			if let session = app.runningSession {
				collapsedRow(session)
				if app.isMiniTrackerExpanded {
					ExpandedTracker(app: app, session: session)
				}
			}
		}
		.padding(EdgeInsets(top: 10, leading: 12, bottom: 10, trailing: 12))
		.frame(width: MiniTrackerLayout.collapsedSize.width, alignment: .topLeading)
		.frame(minHeight: MiniTrackerLayout.collapsedSize.height, maxHeight: .infinity, alignment: .top)
		.background(.regularMaterial, in: RoundedRectangle(cornerRadius: 14))
		.background(Tokens.Colors.surfaceRaised.opacity(0.96), in: RoundedRectangle(cornerRadius: 14))
		.foregroundStyle(Tokens.Colors.ink)
		.textStyle(Tokens.Typography.body)
	}

	private func collapsedRow(_ session: SessionSummary) -> some View {
		HStack(spacing: 10) {
			Circle()
				.fill(app.accentColor)
				.frame(width: 10, height: 10)
				.help(app.workspace?.name ?? "")
				.accessibilityHidden(true)
			VStack(alignment: .leading, spacing: 1) {
				Text(app.sessionText(session))
					.textStyle(Tokens.Typography.body).fontWeight(.semibold)
					.lineLimit(1)
				Text([session.story, session.taskId].compactMap(\.self).joined(separator: " · "))
					.font(Tokens.Typography.caption.font.monospaced())
					.foregroundStyle(Tokens.Colors.inkMuted)
					.lineLimit(1)
			}
			Spacer(minLength: Tokens.Spacing.space4)
			Text(DurationText.clock(seconds: session.elapsed(at: app.timerClock)))
				.textStyle(Tokens.Typography.timer)
				.monospacedDigit()
				.foregroundStyle(app.accentColor)
			TrackerButton(symbol: session.isPaused ? "play.fill" : "pause.fill", label: session.isPaused ? "Resume" : "Pause", help: session.isPaused ? "Resume (Space)" : "Pause (Space)", isPrimary: false) {
				app.togglePauseTimer()
			}
			TrackerButton(symbol: "stop.fill", label: "Stop", help: "Stop and log", isPrimary: true) {
				app.stopTimer()
			}
			Button {
				app.isMiniTrackerExpanded.toggle()
			} label: {
				Image(systemName: app.isMiniTrackerExpanded ? "chevron.up" : "chevron.down")
					.iconReplace(value: app.isMiniTrackerExpanded)
					.iconGlyph(.tag)
					.foregroundStyle(Tokens.Colors.inkMuted)
					.frame(width: 22, height: 24)
			}
			.buttonStyle(.plain)
			.help(app.isMiniTrackerExpanded ? "Collapse (⌥⌘T)" : "Expand (⌥⌘T)")
			.accessibilityLabel(app.isMiniTrackerExpanded ? "Collapse" : "Expand")
		}
		.frame(height: 36)
	}
}

private struct ExpandedTracker: View {
	let app: AppModel
	let session: SessionSummary

	var body: some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space8) {
			if let estimate = app.runningItem?.estimateMinutes, estimate > 0 {
				ProgressView(value: min(1, Double(session.elapsed(at: app.timerClock)) / Double(estimate * 60)))
					.tint(app.accentColor)
					.progressDraw(value: min(1, Double(session.elapsed(at: app.timerClock)) / Double(estimate * 60)))
					.accessibilityLabel("Progress against \(DurationText.minutes(estimate)) planned")
			}
			ForEach(app.runningItem?.subtasks ?? []) { subtask in
				HStack(spacing: Tokens.Spacing.space8) {
					Image(systemName: subtask.isDone ? "checkmark.square" : "square")
						.iconReplace(value: subtask.isDone)
						.foregroundStyle(subtask.isDone ? Tokens.Colors.inkMuted : Tokens.Colors.ink)
					Text(subtask.text)
						.strikethrough(subtask.isDone)
						.foregroundStyle(subtask.isDone ? Tokens.Colors.inkMuted : Tokens.Colors.ink)
						.lineLimit(1)
					Spacer()
					if let minutes = subtask.estimateMinutes {
						Text(DurationText.minutes(minutes))
							.textStyle(Tokens.Typography.caption)
							.foregroundStyle(Tokens.Colors.inkMuted)
					}
				}
				.textStyle(Tokens.Typography.callout)
			}
			if let next = app.nextUpItem {
				Text("Next up · \(next.text)")
					.textStyle(Tokens.Typography.caption)
					.foregroundStyle(Tokens.Colors.inkMuted)
					.lineLimit(1)
			}
			Divider()
			HStack(spacing: 6) {
				Button("Done") { app.finishRunningTask() }
					.disabled(session.taskId == nil)
					.help("Stop the timer and tick the task done")
				Button("Open in Vaultday") { app.openMainWindow(taskId: session.taskId) }
				Spacer()
				Button("Hide tracker") { app.hideMiniTracker() }
					.help("⌥⌘T shows it again")
			}
			.controlSize(.small)
		}
	}
}

private struct TrackerButton: View {
	let symbol: String
	let label: String
	let help: String
	let isPrimary: Bool
	let action: () -> Void

	var body: some View {
		Button(action: action) {
			Image(systemName: symbol)
				.iconGlyph(.tag)
				.iconReplace(value: symbol)
				.foregroundStyle(isPrimary ? Tokens.Colors.onPrimary : Tokens.Colors.ink)
				.frame(width: 28, height: 24)
				.background(isPrimary ? Tokens.Colors.primary : Tokens.Colors.fillControl, in: RoundedRectangle(cornerRadius: 8))
		}
		.buttonStyle(.plain)
		.help(help)
		.accessibilityLabel(label)
	}
}
