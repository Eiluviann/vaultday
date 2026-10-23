import SwiftUI
import VaultdayCore

/// The outcome popover under a signal card: + Task, Wait for, Attach or
/// Snooze, pre-filled from the mail's plain fields only. Save (⏎) writes,
/// Esc cancels; nothing is written until Save.
struct SignalDraftPopover: View {
	let app: AppModel
	@Bindable var inbox: InboxModel
	@FocusState private var focusedField: Field?
	@Environment(\.workspaceAccent) private var accent

	private enum Field: Hashable {
		case text, story, assignee, day, estimate
	}

	var body: some View {
		if let draft = inbox.draft {
			VStack(alignment: .leading, spacing: Tokens.Spacing.space6) {
				HStack(spacing: Tokens.Spacing.space8) {
					Text(draft.title).fontWeight(.semibold)
					Text("· from \(draft.sourceLabel)")
						.foregroundStyle(Tokens.Colors.inkMuted)
						.lineLimit(1)
				}
				fields(for: draft.kind)
				if let section = draft.sectionLabel {
					row("Section") {
						Text(section)
							.font(.custom(Tokens.Typography.mono.family, size: 11))
							.foregroundStyle(Tokens.Colors.inkSecondary)
					}
				}
				HStack(spacing: Tokens.Spacing.space6) {
					Button {
						inbox.saveDraft()
					} label: {
						HStack(spacing: 5) {
							Text("Save")
							Text("⏎").font(Tokens.Typography.kbd.font)
						}
						.fontWeight(.semibold)
						.foregroundStyle(Tokens.Colors.onWs)
						.padding(.horizontal, 10)
						.frame(height: 26)
						.background(accent, in: RoundedRectangle(cornerRadius: 7))
					}
					.buttonStyle(.plain)
					.keyboardShortcut(.defaultAction)
					Button("Cancel") { inbox.cancelDraft() }
						.buttonStyle(.plain)
						.foregroundStyle(Tokens.Colors.inkSecondary)
						.keyboardShortcut(.cancelAction)
					Spacer(minLength: 0)
					Text("Esc cancels · nothing is written until Save")
						.font(Tokens.Typography.caption.font)
						.foregroundStyle(Tokens.Colors.inkMuted)
						.lineLimit(1)
				}
				.padding(.top, 2)
			}
			.font(Tokens.Typography.caption.font)
			.padding(EdgeInsets(top: 10, leading: 12, bottom: 10, trailing: 12))
			.background(Tokens.Colors.surfaceRaised, in: RoundedRectangle(cornerRadius: 10))
			.overlay(RoundedRectangle(cornerRadius: 10).strokeBorder(Tokens.Colors.line))
			.shadow(color: .black.opacity(0.12), radius: 10, y: 6)
			.onExitCommand { inbox.cancelDraft() }
			.onAppear { focusedField = draft.kind == .attach ? .story : draft.kind == .snooze ? nil : .text }
			.onChange(of: focusedField) { app.setTextField("signal-draft", isFocused: focusedField != nil) }
			.onDisappear { app.setTextField("signal-draft", isFocused: false) }
			.accessibilityElement(children: .contain)
			.accessibilityLabel(draft.title)
		}
	}

	@ViewBuilder
	private func fields(for kind: SignalDraftKind) -> some View {
		switch kind {
		case .task:
			textRow("Title", text: draftBinding(\.text), field: .text, placeholder: "Task text")
			storyRow
			textRow("Day", text: draftBinding(\.day), field: .day, placeholder: "today, tomorrow or 2026-10-30")
			textRow("Estimate", text: draftBinding(\.estimate), field: .estimate, placeholder: "45m, 2h")
		case .waitFor:
			textRow("Title", text: draftBinding(\.text), field: .text, placeholder: "What you wait for")
			storyRow
			textRow("Waiting on", text: draftBinding(\.assignedTo), field: .assignee, placeholder: "Who it waits on")
		case .attach:
			storyRow
		case .snooze:
			row("Until") {
				DatePicker("Until", selection: draftBinding(\.snoozeUntil), in: Date()..., displayedComponents: [.date, .hourAndMinute])
					.labelsHidden()
					.datePickerStyle(.field)
			}
		}
	}

	private var storyRow: some View {
		row("Story") {
			HStack(spacing: Tokens.Spacing.space4) {
				field(text: draftBinding(\.story), field: .story, placeholder: "Story number; empty files it in Mail")
				Menu {
					ForEach(inbox.stories) { story in
						Button("\(story.number) · \(story.title)") { inbox.draft?.story = story.number }
					}
				} label: {
					Image(systemName: "chevron.down").font(.system(size: 10))
				}
				.menuStyle(.borderlessButton)
				.menuIndicator(.hidden)
				.fixedSize()
				.accessibilityLabel("Pick a story")
			}
		}
	}

	private func textRow(_ label: String, text: Binding<String>, field: Field, placeholder: String) -> some View {
		row(label) { self.field(text: text, field: field, placeholder: placeholder) }
	}

	private func field(text: Binding<String>, field: Field, placeholder: String) -> some View {
		TextField(placeholder, text: text)
			.textFieldStyle(.plain)
			.focused($focusedField, equals: field)
			.font(Tokens.Typography.callout.font)
			.padding(.horizontal, 8)
			.frame(height: 24)
			.background(Tokens.Colors.surface, in: RoundedRectangle(cornerRadius: 6))
			.overlay(RoundedRectangle(cornerRadius: 6).strokeBorder(focusedField == field ? Tokens.Colors.focus : Tokens.Colors.lineControl))
			.onSubmit { inbox.saveDraft() }
	}

	private func row(_ label: String, @ViewBuilder content: () -> some View) -> some View {
		HStack(spacing: Tokens.Spacing.space8) {
			Text(label)
				.foregroundStyle(Tokens.Colors.inkMuted)
				.frame(width: 62, alignment: .leading)
			content()
				.frame(maxWidth: .infinity, alignment: .leading)
		}
		.frame(minHeight: 24)
	}

	private func draftBinding<Value>(_ keyPath: WritableKeyPath<SignalDraft, Value>) -> Binding<Value> where Value: Equatable {
		Binding(
			get: { inbox.draft.map { $0[keyPath: keyPath] } ?? Self.placeholderDraft[keyPath: keyPath] },
			set: { inbox.draft?[keyPath: keyPath] = $0 },
		)
	}

	/// What a binding reads in the instant between Save and the popover closing.
	private static let placeholderDraft = SignalDraft(
		signalId: "",
		kind: .task,
		text: "",
		story: "",
		assignedTo: "",
		day: "",
		estimate: "",
		snoozeUntil: Date(),
	)
}
