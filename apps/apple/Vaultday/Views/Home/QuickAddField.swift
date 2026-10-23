import SwiftUI
import VaultdayCore

/// Quick add: one field and Add. Add or ⏎ opens the confirmation under the
/// field with what was understood; nothing is written until Save.
struct QuickAddField: View {
	let app: AppModel
	@Bindable var quickAdd: QuickAddModel
	@FocusState private var isFocused: Bool
	@Environment(\.workspaceAccent) private var accent

	var body: some View {
		HStack(spacing: Tokens.Spacing.space8) {
			VimTextField("Add a task · a day, a time, an estimate and a story route it", text: $quickAdd.text, onEscape: closeConfirmation)
				.textFieldStyle(.plain)
				.font(Tokens.Typography.body.font)
				.padding(.horizontal, 10)
				.frame(height: 32)
				.background(Tokens.Colors.surfaceRaised, in: RoundedRectangle(cornerRadius: Tokens.Radius.radiusMd))
				.overlay(RoundedRectangle(cornerRadius: Tokens.Radius.radiusMd).strokeBorder(isFocused ? Tokens.Colors.focus : Tokens.Colors.lineButton))
				.focused($isFocused)
				.onSubmit(submit)
				.onExitCommand {
					if quickAdd.entry != nil { quickAdd.cancel() } else { isFocused = false }
				}
				.accessibilityLabel("Add a task")
				.help(app.keyLabel(.addTask).map { "Natural language: a day, a time, an estimate and a story number or name route it; Add shows what was understood first (\($0) from the list)" } ?? "Natural language: a day, a time, an estimate and a story number or name route it")
			Button(action: submit) {
				Text("Add")
					.fontWeight(.semibold)
					.foregroundStyle(Tokens.Colors.onWs)
					.padding(.horizontal, 12)
					.frame(height: 28)
					.background(accent, in: RoundedRectangle(cornerRadius: Tokens.Radius.radiusMd))
			}
			.buttonStyle(.plain)
			.disabled(quickAdd.text.trimmingCharacters(in: .whitespaces).isEmpty)
		}
		.overlay(alignment: .topLeading) {
			if let entry = quickAdd.entry {
				Confirmation(app: app, quickAdd: quickAdd, entry: entry)
					.padding(.trailing, 52)
					.offset(y: 38)
			} else if let message = quickAdd.message {
				Text(message)
					.font(Tokens.Typography.caption.font)
					.foregroundStyle(Tokens.Colors.caution)
					.offset(y: 36)
			}
		}
		.onChange(of: app.quickAddFocusRequest) { isFocused = true }
		.onChange(of: isFocused) { app.setTextField("quick-add", isFocused: isFocused) }
		// ⌘0 or a workspace switch can take the field away while it has focus; no change of isFocused follows.
		.onDisappear { app.setTextField("quick-add", isFocused: false) }
		.onChange(of: quickAdd.text) {
			// What was confirmed is no longer what the field says.
			if quickAdd.entry != nil, !quickAdd.isAdjusting { quickAdd.cancel() }
		}
	}

	/// Esc in the field's Normal mode closes an open confirmation first and
	/// keeps the field; `false` lets the field give the keys up.
	private func closeConfirmation() -> Bool {
		guard quickAdd.entry != nil else { return false }
		quickAdd.cancel()
		return true
	}

	/// ⏎ in the field: Add opens the confirmation; with it open, Save.
	private func submit() {
		if quickAdd.entry != nil {
			app.saveQuickAdd()
		} else {
			quickAdd.submit()
		}
	}
}

/// What the grammar understood: kind, text, story, day, estimate, section, id.
private struct Confirmation: View {
	let app: AppModel
	let quickAdd: QuickAddModel
	let entry: QuickAddEntry
	@Environment(\.workspaceAccent) private var accent

	var body: some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space6) {
			HStack(spacing: Tokens.Spacing.space8) {
				Text(entry.kindTitle).fontWeight(.bold)
				Button {
					quickAdd.switchKind()
				} label: {
					Text("· \(entry.switchTitle)")
						.foregroundStyle(Tokens.Colors.inkMuted)
				}
				.buttonStyle(.plain)
				Spacer()
				Text(entry.id)
					.font(.custom(Tokens.Typography.mono.family, size: 11))
					.foregroundStyle(Tokens.Colors.inkMuted)
					.textSelection(.enabled)
			}
			if quickAdd.isAdjusting {
				AdjustFields(app: app, quickAdd: quickAdd, entry: entry)
			} else {
				Text(entry.text)
					.fontWeight(.semibold)
				HStack(spacing: Tokens.Spacing.space6) {
					ForEach(Array(entry.chips.enumerated()), id: \.offset) { index, chip in
						Text(chip)
							.font(index == 0 && entry.storyNumber != nil ? .custom(Tokens.Typography.mono.family, size: 11) : Tokens.Typography.caption.font)
							.padding(.horizontal, Tokens.Spacing.space8)
							.padding(.vertical, Tokens.Spacing.space2)
							.background(Tokens.Colors.fillControl, in: Capsule())
					}
				}
			}
			ForEach(entry.warnings, id: \.self) { warning in
				Label(warning, systemImage: "exclamationmark.triangle")
					.font(Tokens.Typography.caption.font)
					.foregroundStyle(Tokens.Colors.caution)
			}
			HStack(spacing: Tokens.Spacing.space6) {
				Button {
					app.saveQuickAdd()
				} label: {
					HStack(spacing: 5) {
						Text("Save").fontWeight(.semibold)
						Text("⏎")
							.font(Tokens.Typography.kbd.font)
							.padding(.horizontal, 4)
							.padding(.vertical, 2)
							.background(Tokens.Colors.onWs.opacity(0.28), in: RoundedRectangle(cornerRadius: Tokens.Radius.radiusXs))
					}
					.foregroundStyle(Tokens.Colors.onWs)
					.padding(.horizontal, 10)
					.frame(height: 26)
					.background(entry.canSave ? accent : Tokens.Colors.inkDisabled, in: RoundedRectangle(cornerRadius: 7))
				}
				.buttonStyle(.plain)
				.disabled(!entry.canSave)
				Button {
					quickAdd.adjust()
				} label: {
					Text("Adjust")
						.padding(.horizontal, 10)
						.frame(height: 26)
						.background(Tokens.Colors.surfaceRaised, in: RoundedRectangle(cornerRadius: 7))
						.overlay(RoundedRectangle(cornerRadius: 7).strokeBorder(Tokens.Colors.lineButton))
				}
				.buttonStyle(.plain)
				.disabled(quickAdd.isAdjusting)
				Spacer()
				Text(quickAdd.message ?? entry.saveBlocker ?? cancelHint)
					.font(Tokens.Typography.caption.font)
					.foregroundStyle(entry.saveBlocker == nil && quickAdd.message == nil ? Tokens.Colors.inkMuted : Tokens.Colors.caution)
					.lineLimit(2)
			}
			.padding(.top, Tokens.Spacing.space2)
		}
		.font(Tokens.Typography.callout.font)
		.foregroundStyle(Tokens.Colors.ink)
		.padding(EdgeInsets(top: 10, leading: 12, bottom: 10, trailing: 12))
		.background(Tokens.Colors.surfaceRaised, in: RoundedRectangle(cornerRadius: 10))
		.overlay(RoundedRectangle(cornerRadius: 10).strokeBorder(Tokens.Colors.line))
		.shadow(color: .black.opacity(0.14), radius: 16, y: 12)
		.accessibilityElement(children: .contain)
		.accessibilityLabel("Confirm what was understood")
	}

	private var cancelHint: String {
		app.keyLabel(.dismiss).map { "\($0) cancels · nothing is written until Save" } ?? "Nothing is written until Save"
	}
}

/// Adjust: the chips as fields.
private struct AdjustFields: View {
	enum Field: Hashable { case text, estimate, waitingOn }

	let app: AppModel
	let quickAdd: QuickAddModel
	let entry: QuickAddEntry
	@State private var estimateText = ""
	@State private var waitingOn = ""
	@FocusState private var focusedField: Field?

	var body: some View {
		Grid(alignment: .leading, horizontalSpacing: Tokens.Spacing.space8, verticalSpacing: Tokens.Spacing.space6) {
			GridRow {
				Text("Text").foregroundStyle(Tokens.Colors.inkMuted)
				VimTextField("What to do", text: Binding(get: { entry.text }, set: { quickAdd.setText($0) }))
					.focused($focusedField, equals: .text)
			}
			GridRow {
				Text("Story").foregroundStyle(Tokens.Colors.inkMuted)
				Picker("Story", selection: Binding(
					get: { entry.isStoryKnown ? entry.storyNumber : nil },
					set: { number in quickAdd.setStory(quickAdd.stories.first { $0.number == number }) },
				)) {
					Text("Pick a story").tag(String?.none)
					ForEach(quickAdd.stories) { story in
						Text("\(story.number) · \(story.title)").tag(String?.some(story.number))
					}
				}
				.labelsHidden()
			}
			GridRow {
				Text("Day").foregroundStyle(Tokens.Colors.inkMuted)
				DatePicker("Day", selection: Binding(
					get: { DayText.date(from: entry.day) ?? Date() },
					set: { quickAdd.setDay($0) },
				), displayedComponents: .date)
					.labelsHidden()
			}
			GridRow {
				Text("Estimate").foregroundStyle(Tokens.Colors.inkMuted)
				VimTextField("45m", text: $estimateText)
					.focused($focusedField, equals: .estimate)
					.onSubmit { quickAdd.setEstimate(estimateText) }
					.onChange(of: estimateText) { quickAdd.setEstimate(estimateText) }
			}
			GridRow {
				Text("Section").foregroundStyle(Tokens.Colors.inkMuted)
				HStack {
					Picker("Section", selection: Binding(
						get: { entry.isPendingInput },
						set: { quickAdd.setPendingInput($0, waitingOn: waitingOn) },
					)) {
						Text("Next Steps").tag(false)
						Text("Pending Input").tag(true)
					}
					.labelsHidden()
					.pickerStyle(.segmented)
					if entry.isPendingInput {
						VimTextField("Waiting on", text: $waitingOn)
							.focused($focusedField, equals: .waitingOn)
							.onChange(of: waitingOn) { quickAdd.setPendingInput(true, waitingOn: waitingOn) }
					}
				}
			}
		}
		.textFieldStyle(.roundedBorder)
		.onChange(of: focusedField) { app.setTextField("adjust", isFocused: focusedField != nil) }
		.onDisappear { app.setTextField("adjust", isFocused: false) }
		.onAppear {
			estimateText = entry.estimateMinutes.map(DurationText.minutes) ?? ""
			waitingOn = entry.waitingOn ?? ""
		}
	}
}
