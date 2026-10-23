import AppKit
import SwiftUI
import VaultdayCore

/// A task card of the Home board: the title line (grip, checkbox, title, slot,
/// estimate), the story line (story, waiting, more actions) and the subtask
/// line (subtasks, + subtask, the id). A done task is one muted line.
struct TaskCard: View {
	let app: AppModel
	let item: PlanItem
	let isSelected: Bool
	/// The All view's cards: no ticking, no dragging, a workspace dot.
	var readOnlyColor: Color?

	@State private var isHovered = false
	@Environment(\.workspaceAccent) private var accent

	private var isReadOnly: Bool { readOnlyColor != nil }
	private var tint: Color { readOnlyColor ?? accent }

	var body: some View {
		Group {
			if item.isDone {
				doneLine
			} else {
				VStack(alignment: .leading, spacing: 0) {
					titleLine
					storyLine
					if item.subtasks.isEmpty {
						subtaskFoot
							.padding(EdgeInsets(top: 1, leading: 34, bottom: 6, trailing: 10))
					} else {
						subtaskBlock
					}
				}
				.background(Tokens.Colors.surface)
				.clipShape(RoundedRectangle(cornerRadius: 10))
				.overlay(RoundedRectangle(cornerRadius: 10).strokeBorder(isSelected ? tint : Tokens.Colors.line, lineWidth: isSelected ? 2 : 1))
			}
		}
		.lift(isHovered || isSelected)
		.contentShape(Rectangle())
		.onHover { hovering in
			isHovered = hovering
			if hovering, !isReadOnly { app.hover(item) }
		}
		.onTapGesture(count: 2) {
			if !isReadOnly { app.openPreview(taskId: item.taskId) }
		}
		.onTapGesture {
			if !isReadOnly { app.select(item) }
		}
		.peekPopover(app: app, subject: isReadOnly ? nil : .task(item.taskId))
		.modifier(DraggableTask(taskId: item.taskId, isEnabled: !isReadOnly && !item.isDone))
		.accessibilityElement(children: .contain)
		.accessibilityAddTraits(isSelected ? .isSelected : [])
	}

	// MARK: Lines

	/// Grip, checkbox, title, slot and estimate on one baseline: a wrapped
	/// title keeps its first line beside the checkbox.
	private var titleLine: some View {
		HStack(alignment: .firstTextBaseline, spacing: 10) {
			if let readOnlyColor {
				Circle().fill(readOnlyColor).frame(width: 8, height: 8)
			} else {
				Text("⋮⋮")
					.font(.system(size: 14))
					.tracking(-2)
					.foregroundStyle(Tokens.Colors.lineControl)
					.accessibilityHidden(true)
			}
			TaskCheckbox(isDone: false, tint: tint, size: 16, label: "Done: \(item.text)", isEnabled: !isReadOnly) {
				app.completeTask(item.taskId)
			}
			Text(item.text)
				.fontWeight(item.state == .inProgress ? .semibold : .regular)
				.foregroundStyle(Tokens.Colors.ink)
				.frame(maxWidth: .infinity, alignment: .leading)
				.fixedSize(horizontal: false, vertical: true)
			if let slot = item.slot {
				SlotChip(slot: slot)
			}
			estimate
		}
		.font(Tokens.Typography.body.font)
		.lineSpacing(2)
		.padding(EdgeInsets(top: 9, leading: 6, bottom: 2, trailing: 10))
	}

	@ViewBuilder
	private var estimate: some View {
		if let minutes = item.estimateMinutes {
			Text(DurationText.minutes(minutes))
				.font(Tokens.Typography.callout.font.weight(.semibold))
				.foregroundStyle(item.state == .inProgress ? tint : Tokens.Colors.inkMuted)
		} else {
			Text("no estimate")
				.font(Tokens.Typography.callout.font.weight(.semibold))
				.foregroundStyle(Tokens.Colors.caution)
		}
	}

	private var storyLine: some View {
		HStack(spacing: Tokens.Spacing.space6) {
			if let number = item.storyNumber {
				MonoChip(text: number)
					.help(item.storyTitle ?? number)
			}
			if let days = item.waitingDays, item.isPendingInput {
				CautionChip(text: item.assignedTo.map { "\($0) · waiting \(days)d" } ?? "waiting \(days)d")
			}
			Spacer()
			if !isReadOnly {
				MoreActions(app: app, taskId: item.taskId, canMove: true)
			}
		}
		.padding(EdgeInsets(top: 4, leading: 34, bottom: 3, trailing: 10))
	}

	private var subtaskFoot: some View {
		HStack(spacing: Tokens.Spacing.space4) {
			Label("subtask", systemImage: "plus")
				.labelStyle(TightLabelStyle())
				.font(Tokens.Typography.caption.font)
				.foregroundStyle(Tokens.Colors.inkDisabled)
				.help("Adding subtasks from Home arrives in a later version")
			Spacer()
			MonoChip(text: item.taskId, isRounded: false)
				.help("Readable id")
		}
		.frame(minHeight: 20)
	}

	private var subtaskBlock: some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space2) {
			ForEach(item.subtasks) { subtask in
				SubtaskRow(app: app, subtask: subtask, tint: tint, isReadOnly: isReadOnly)
			}
			subtaskFoot
		}
		.padding(EdgeInsets(top: 4, leading: 34, bottom: 6, trailing: 10))
		.frame(maxWidth: .infinity, alignment: .leading)
		.background(Tokens.Colors.surfaceSunken)
		.overlay(alignment: .top) { Rectangle().fill(Tokens.Colors.lineFaint).frame(height: 1) }
	}

	private var doneLine: some View {
		HStack(alignment: .firstTextBaseline, spacing: 10) {
			Text("⋮⋮")
				.font(.system(size: 14))
				.tracking(-2)
				.foregroundStyle(Tokens.Colors.line)
				.accessibilityHidden(true)
			TaskCheckbox(isDone: true, tint: tint, size: 16, label: "Done: \(item.text)", isEnabled: false) {}
			Text(item.text)
				.strikethrough()
				.frame(maxWidth: .infinity, alignment: .leading)
			if let minutes = item.estimateMinutes {
				Text(DurationText.minutes(minutes))
					.font(Tokens.Typography.callout.font)
			}
		}
		.font(Tokens.Typography.body.font)
		.foregroundStyle(Tokens.Colors.inkMuted)
		.padding(EdgeInsets(top: 10, leading: 6, bottom: 10, trailing: 10))
		.overlay(RoundedRectangle(cornerRadius: 10).strokeBorder(Tokens.Colors.line))
	}
}

/// A subtask line; play or pause and the more menu slide in from the right on hover.
private struct SubtaskRow: View {
	let app: AppModel
	let subtask: PlanSubtask
	let tint: Color
	let isReadOnly: Bool
	@State private var isHovered = false

	private var isRunning: Bool {
		guard let session = app.sessions.openSession, let taskId = subtask.taskId else { return false }
		return session.taskId == taskId
	}

	var body: some View {
		HStack(spacing: Tokens.Spacing.space8) {
			TaskCheckbox(isDone: subtask.isDone, tint: tint, size: 14, label: "Done: \(subtask.text)", isEnabled: !isReadOnly && !subtask.isDone && subtask.taskId != nil) {
				if let taskId = subtask.taskId { app.completeTask(taskId) }
			}
			Text(subtask.text)
				.fontWeight(isRunning ? .medium : .regular)
				.strikethrough(subtask.isDone)
				.foregroundStyle(subtask.isDone ? Tokens.Colors.inkMuted : Tokens.Colors.ink)
				.frame(maxWidth: .infinity, alignment: .leading)
			if isRunning, let session = app.sessions.openSession {
				TimelineView(.periodic(from: .now, by: 1)) { context in
					Text(session.isPaused ? "paused" : "running \(DurationText.clock(seconds: session.elapsed(at: context.date)).dropFirst(2))")
				}
				.font(Tokens.Typography.caption.font.weight(.semibold))
				.foregroundStyle(tint)
			}
			if let minutes = subtask.estimateMinutes {
				Text(DurationText.minutes(minutes))
					.font(Tokens.Typography.caption.font)
					.foregroundStyle(Tokens.Colors.inkMuted)
			}
		}
		.font(Tokens.Typography.body.font)
		.padding(.vertical, Tokens.Spacing.space4)
		.overlay(alignment: .trailing) {
			if !isReadOnly, isHovered || isRunning {
				controls
					.transition(.move(edge: .trailing).combined(with: .opacity))
			}
		}
		.animation(.easeOut(duration: 0.15), value: isHovered)
		.onHover { isHovered = $0 }
	}

	private var controls: some View {
		HStack(spacing: Tokens.Spacing.space4) {
			if let taskId = subtask.taskId, !subtask.isDone {
				if isRunning, let session = app.sessions.openSession {
					CardIconButton(symbol: session.isPaused ? "play.fill" : "pause.fill", label: session.isPaused ? "Resume timer" : "Pause timer") {
						app.sessions.togglePause()
					}
				} else {
					CardIconButton(symbol: "play.fill", label: "Start timer on this subtask") {
						app.startTimer(taskId: taskId)
					}
				}
			}
			if let taskId = subtask.taskId {
				MoreActions(app: app, taskId: taskId, canMove: false)
			}
		}
		.padding(.leading, 28)
		.background(LinearGradient(stops: [.init(color: Tokens.Colors.surfaceSunken.opacity(0), location: 0), .init(color: Tokens.Colors.surfaceSunken, location: 0.4)], startPoint: .leading, endPoint: .trailing))
	}
}

/// The ⋯ menu of a card or a subtask.
private struct MoreActions: View {
	let app: AppModel
	let taskId: String
	let canMove: Bool

	var body: some View {
		Menu {
			Button("Start timer") { app.startTimer(taskId: taskId) }
			if canMove {
				Button("Move up") { moveAndKeep(-1) }
				Button("Move down") { moveAndKeep(1) }
			}
			Divider()
			Button("Copy id") {
				NSPasteboard.general.clearContents()
				NSPasteboard.general.setString(taskId, forType: .string)
			}
		} label: {
			Image(systemName: "ellipsis")
				.font(.system(size: 12))
				.frame(minWidth: 24, minHeight: 20)
				.foregroundStyle(Tokens.Colors.inkSecondary)
				.background(Tokens.Colors.surfaceRaised, in: RoundedRectangle(cornerRadius: Tokens.Radius.radiusSm))
				.overlay(RoundedRectangle(cornerRadius: Tokens.Radius.radiusSm).strokeBorder(Tokens.Colors.lineButton))
		}
		.menuStyle(.button)
		.menuIndicator(.hidden)
		.buttonStyle(.plain)
		.fixedSize()
		.help("More actions")
		.accessibilityLabel("More actions")
	}

	private func moveAndKeep(_ step: Int) {
		guard let index = app.dayPlan.items.firstIndex(where: { $0.taskId == taskId }) else { return }
		app.moveTask(taskId, to: index + step)
	}
}

/// A square checkbox in the workspace color.
private struct TaskCheckbox: View {
	let isDone: Bool
	let tint: Color
	let size: CGFloat
	let label: String
	let isEnabled: Bool
	let action: () -> Void

	var body: some View {
		Button(action: action) {
			Image(systemName: isDone ? "checkmark.square.fill" : "square")
				.font(.system(size: size))
				.foregroundStyle(isDone ? tint : Tokens.Colors.lineControl)
		}
		.buttonStyle(.plain)
		.disabled(!isEnabled)
		.accessibilityLabel(label)
		.accessibilityValue(isDone ? "Done" : "Open")
	}
}

/// The time box the scheduler laid: a solid chip when locked, dashed while it is a projection.
private struct SlotChip: View {
	let slot: PlanSlot

	var body: some View {
		Text(slot.isLocked ? slot.start.text : "projected \(slot.start.text)")
			.font(Tokens.Typography.callout.font)
			.foregroundStyle(slot.isLocked ? Tokens.Colors.ink : Tokens.Colors.inkMuted)
			.padding(.horizontal, Tokens.Spacing.space8)
			.padding(.vertical, Tokens.Spacing.space2)
			.background(slot.isLocked ? Tokens.Colors.fillControl : .clear, in: RoundedRectangle(cornerRadius: Tokens.Radius.radiusSm))
			.overlay {
				if !slot.isLocked {
					RoundedRectangle(cornerRadius: Tokens.Radius.radiusSm)
						.strokeBorder(Tokens.Colors.lineButton, style: StrokeStyle(lineWidth: 1, dash: [3, 2]))
				}
			}
			.help("\(DurationText.minutes(slot.minutes)) from \(slot.start.text)")
	}
}

/// A drag source carrying the task id, onto another card or the Now card.
private struct DraggableTask: ViewModifier {
	let taskId: String
	let isEnabled: Bool

	func body(content: Content) -> some View {
		if isEnabled {
			content.draggable(taskId)
		} else {
			content
		}
	}
}

struct TightLabelStyle: LabelStyle {
	func makeBody(configuration: Configuration) -> some View {
		HStack(spacing: 4) {
			configuration.icon.font(.system(size: 8, weight: .semibold))
			configuration.title
		}
	}
}
