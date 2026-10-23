import SwiftUI
import VaultdayCore

/// Next Steps as an accordion in the note's order, done steps in place: a
/// fixed-height scroll area that opens at the first open step, with a
/// caption counting the completed steps above it. Only the selected or the
/// hovered step opens to show its controls; ticking, scheduling or
/// re-estimating a step rewrites that line only.
struct NextStepsSection: View {
	let app: AppModel
	let story: StoryModel

	var body: some View {
		let accordion = story.accordion
		VStack(alignment: .leading, spacing: Tokens.Spacing.space6) {
			HStack(spacing: Tokens.Spacing.space8) {
				Text("In the note's order · one step open at a time")
					.textStyle(Tokens.Typography.caption)
					.foregroundStyle(Tokens.Colors.inkMuted)
				Spacer()
				if let caption = accordion.completedCaption {
					Label(caption, systemImage: "arrow.up")
						.textStyle(Tokens.Typography.caption)
						.foregroundStyle(Tokens.Colors.inkMuted)
				}
			}
			if accordion.steps.isEmpty {
				Text("No next steps in the note")
					.textStyle(Tokens.Typography.callout)
					.foregroundStyle(Tokens.Colors.inkMuted)
			} else {
				ScrollViewReader { proxy in
					ScrollView {
						VStack(alignment: .leading, spacing: 2) {
							ForEach(Array(accordion.steps.enumerated()), id: \.element.id) { index, step in
								StepRow(app: app, story: story, step: step, isExpanded: accordion.showsControls(step))
									.id(step.id)
									.onHover { isInside in story.hoverStep(id: isInside ? step.id : nil) }
									.onTapGesture { app.selectStoryStep(step) }
									.accessibilityAddTraits(accordion.selectedIndex == index ? .isSelected : [])
							}
						}
						.padding(Tokens.Spacing.space4)
					}
					.frame(height: StoryLayout.accordionHeight)
					.onAppear {
						if let first = accordion.initialScrollId { proxy.scrollTo(first, anchor: .top) }
					}
					.onChange(of: accordion.selectedId) {
						if let selected = accordion.selectedId {
							withAnimation(.easeOut(duration: Tokens.Motion.motionTransition)) { proxy.scrollTo(selected) }
						}
					}
				}
				.background(Tokens.Colors.surfaceSunken, in: RoundedRectangle(cornerRadius: Tokens.Radius.radiusMd))
				.overlay(RoundedRectangle(cornerRadius: Tokens.Radius.radiusMd).strokeBorder(Tokens.Colors.line))
			}
		}
	}
}

/// One step: a line folded; with its controls when selected or hovered.
private struct StepRow: View {
	let app: AppModel
	let story: StoryModel
	let step: StoryStep
	let isExpanded: Bool

	var body: some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space6) {
			HStack(spacing: Tokens.Spacing.space8) {
				StoryCheckboxButton(checkbox: StoryCheckbox(step: step)) {
					guard let taskId = step.taskId else { return }
					story.toggle(taskId: taskId, isDone: step.isDone)
					app.storyChanged()
				}
				Text(step.text)
					.font(Tokens.Typography.body.font.weight(step.state == .inProgress ? .semibold : .regular))
					.strikethrough(step.isDone)
					.foregroundStyle(step.isDone ? Tokens.Colors.inkMuted : Tokens.Colors.ink)
					.lineLimit(isExpanded ? nil : 1)
					.frame(maxWidth: .infinity, alignment: .leading)
				if let completion = step.completionLabel {
					meta("done \(completion)")
				} else if let scheduled = step.scheduled {
					meta(story.scheduledLabel(scheduled))
				}
				if let estimate = step.estimateLabel {
					meta(estimate)
				}
			}
			if isExpanded {
				HStack(spacing: Tokens.Spacing.space8) {
					if let taskId = step.taskId, !step.isDone {
						scheduleMenu(taskId)
						estimateMenu(taskId)
						Button {
							app.startTimer(taskId: taskId)
						} label: {
							Label("Start timer", systemImage: "play")
						}
						.buttonStyle(.borderless)
					}
					if let due = step.dueLabel {
						meta("due \(due)")
					}
					Spacer()
					if let taskId = step.taskId {
						Text(taskId)
							.textStyle(Tokens.Typography.mono)
							.foregroundStyle(Tokens.Colors.inkMuted)
							.textSelection(.enabled)
					} else {
						meta("no id, change it in Obsidian")
					}
				}
				.textStyle(Tokens.Typography.callout)
				.padding(.leading, 22)
			}
		}
		.padding(.horizontal, Tokens.Spacing.space8)
		.padding(.vertical, Tokens.Spacing.space6)
		.padding(.leading, CGFloat(step.depth) * Tokens.Spacing.space20)
		.background(isExpanded ? Tokens.Colors.surfaceRaised : .clear, in: RoundedRectangle(cornerRadius: Tokens.Radius.radiusSm))
		.contentShape(Rectangle())
		.animation(.easeOut(duration: Tokens.Motion.motionTransition), value: isExpanded)
	}

	private func meta(_ text: String) -> some View {
		Text(text)
			.textStyle(Tokens.Typography.caption)
			.foregroundStyle(Tokens.Colors.inkMuted)
			.lineLimit(1)
	}

	private func scheduleMenu(_ taskId: String) -> some View {
		let choices = story.scheduleChoices
		return Menu {
			Button("Today") { change { story.schedule(taskId: taskId, on: choices.today) } }
			Button("Tomorrow") { change { story.schedule(taskId: taskId, on: choices.tomorrow) } }
			if step.scheduled != nil {
				Divider()
				Button("Remove the day") { change { story.schedule(taskId: taskId, on: nil) } }
			}
		} label: {
			Label(step.scheduled.map(story.scheduledLabel) ?? "Schedule", systemImage: "calendar")
		}
		.menuStyle(.borderlessButton)
		.fixedSize()
		.help("Writes [scheduled:: ] on this line only")
	}

	private func estimateMenu(_ taskId: String) -> some View {
		Menu {
			ForEach(["15m", "30m", "45m", "1h", "1h30m", "2h", "3h", "4h"], id: \.self) { value in
				Button(value) { change { story.estimate(taskId: taskId, to: value) } }
			}
			if step.estimateMinutes != nil {
				Divider()
				Button("Remove the estimate") { change { story.estimate(taskId: taskId, to: nil) } }
			}
		} label: {
			Label(step.estimateLabel ?? "Estimate", systemImage: "hourglass")
		}
		.menuStyle(.borderlessButton)
		.fixedSize()
		.help("Writes [estimate:: ] on this line only")
	}

	private func change(_ edit: () -> Void) {
		edit()
		app.storyChanged()
	}
}

/// Pending Input in the note's order: what the story waits on, from whom.
struct PendingInputList: View {
	let app: AppModel
	let story: StoryModel
	let content: StoryContent

	var body: some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space4) {
			Text("Waiting on others")
				.textStyle(Tokens.Typography.caption)
				.foregroundStyle(Tokens.Colors.inkMuted)
			if content.pendingInput.isEmpty {
				Text("Nothing waits on anyone")
					.textStyle(Tokens.Typography.callout)
					.foregroundStyle(Tokens.Colors.inkMuted)
			}
			ForEach(content.pendingInput) { step in
				HStack(spacing: Tokens.Spacing.space8) {
					StoryCheckboxButton(checkbox: StoryCheckbox(step: step)) {
						guard let taskId = step.taskId else { return }
						story.toggle(taskId: taskId, isDone: step.isDone)
						app.storyChanged()
					}
					Text(step.text)
						.strikethrough(step.isDone)
						.foregroundStyle(step.isDone ? Tokens.Colors.inkMuted : Tokens.Colors.ink)
						.frame(maxWidth: .infinity, alignment: .leading)
					if let waiting = content.waiting.first(where: { $0.taskId != nil && $0.taskId == step.taskId }) {
						CautionChip(text: waiting.detail)
					} else if let person = step.assignedTo {
						Text(step.completionLabel.map { "\(person) · answered \($0)" } ?? person)
							.textStyle(Tokens.Typography.caption)
							.foregroundStyle(Tokens.Colors.inkMuted)
					}
				}
				.textStyle(Tokens.Typography.body)
				.padding(.leading, CGFloat(step.depth) * Tokens.Spacing.space20)
			}
		}
	}
}
