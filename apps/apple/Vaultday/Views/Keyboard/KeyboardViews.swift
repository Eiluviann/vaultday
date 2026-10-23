import SwiftUI
import VaultdayCore

// The keyboard model on screen: the showcmd indicator, the pane numbers
// while `g` is pending, the shortcut caps and which-key completions on the
// selected item, the item's menu, and the change editor. Every key text
// comes from the dispatcher, so these never disagree with Keymap.yaml.

/// A key cap; `isLit` marks a key that completes the pending strokes.
struct KeyCap: View {
	let text: AttributedString
	var isLit = false
	var isMuted = false

	init(text: String, isLit: Bool = false, isMuted: Bool = false) {
		self.init(attributed: AttributedString(text), isLit: isLit, isMuted: isMuted)
	}

	init(attributed: AttributedString, isLit: Bool = false, isMuted: Bool = false) {
		text = attributed
		self.isLit = isLit
		self.isMuted = isMuted
	}

	var body: some View {
		Text(text)
			.font(Tokens.Typography.kbd.font.weight(isLit ? .bold : .semibold))
			.foregroundStyle(isMuted ? Tokens.Colors.inkMuted : Tokens.Colors.ink)
			.padding(.horizontal, 5)
			.padding(.vertical, 2)
			.frame(minWidth: 14)
			.background(isLit ? Tokens.Colors.sunriseSoft : Tokens.Colors.fillControl, in: RoundedRectangle(cornerRadius: Tokens.Radius.radiusXs))
			.overlay {
				if isLit {
					RoundedRectangle(cornerRadius: Tokens.Radius.radiusXs).strokeBorder(Tokens.Colors.sunrise)
				}
			}
			.shadow(color: isLit ? Tokens.Colors.sunrise.opacity(0.25) : .clear, radius: 2)
	}
}

/// One key sequence as one hint box, `g1` rather than `g` and `1` apart;
/// while strokes are pending the box is lit and the typed prefix is
/// highlighted inside it, as which-key draws `[**g**1]`.
struct SequenceCaps: View {
	let caps: [String]
	/// How many leading strokes are already typed.
	var typed = 0

	var body: some View {
		KeyCap(attributed: Self.text(caps, typed: typed), isLit: typed > 0)
	}

	/// The strokes run together, a thin space only beside a named key (`Space`, `Esc`).
	static func text(_ caps: [String], typed: Int) -> AttributedString {
		var text = AttributedString()
		for (index, cap) in caps.enumerated() {
			if index > 0, cap.count > 1 || caps[index - 1].count > 1 {
				text += AttributedString("\u{2009}")
			}
			var stroke = AttributedString(cap)
			if index < typed {
				stroke.foregroundColor = Tokens.Colors.focus
				stroke.font = Tokens.Typography.kbd.font.weight(.heavy)
			}
			text += stroke
		}
		return text
	}
}

/// The pending strokes bottom right, as vim's showcmd, with a sunrise cursor.
struct ShowCommand: View {
	let dispatcher: KeyDispatcher

	var body: some View {
		if !dispatcher.pendingText.isEmpty {
			HStack(spacing: 0) {
				Text(dispatcher.pendingText)
				Text("▍").foregroundStyle(Tokens.Colors.sunrise)
			}
			.font(Tokens.Typography.mono.font.weight(.semibold))
			.foregroundStyle(Tokens.Colors.onPrimary)
			.padding(.horizontal, Tokens.Spacing.space8)
			.padding(.vertical, 5)
			.background(Tokens.Colors.primary, in: RoundedRectangle(cornerRadius: 6))
			.padding(Tokens.Spacing.space12)
			.help("Pending keys; they wait for the next key, and Esc cancels them")
			.accessibilityLabel("Pending keys \(dispatcher.pendingText)")
		}
	}
}

/// A pane's number in its corner while a pending `g` can jump to it.
struct PaneNumberBadge: View {
	let dispatcher: KeyDispatcher
	let pane: Pane

	var body: some View {
		if let completion = dispatcher.completions.first(where: { $0.action == .focusPane(pane) }) {
			SequenceCaps(caps: completion.sequence.map(\.capText), typed: dispatcher.pending.count)
				.padding(Tokens.Spacing.space8)
			.transition(.opacity)
			.accessibilityLabel("Pane \(completion.next.capText)")
		}
	}
}

extension View {
	/// The pane's number while `g` is pending.
	func paneNumber(_ pane: Pane, dispatcher: KeyDispatcher) -> some View {
		overlay(alignment: .topTrailing) { PaneNumberBadge(dispatcher: dispatcher, pane: pane) }
	}
}

/// The single keys that act on the selected item, or, while a stroke is
/// pending, only the keys that complete it, lit. They arrive and leave
/// with the selection through the grow transition, or at once under the
/// Instant setting, or as a crossfade under Reduce Motion.
struct SelectionCaps: View {
	let app: AppModel
	@Environment(\.accessibilityReduceMotion) private var reduceMotion

	/// The keys the selected card shows when nothing is pending.
	private static let itemActions: [KeyAction] = [.toggleTimer, .completeTask, .togglePin, .peek, .yank, .cut]

	var body: some View {
		let dispatcher = app.dispatcher
		HStack(spacing: Tokens.Spacing.space8) {
			if dispatcher.completions.isEmpty {
				ForEach(Self.itemActions, id: \.self) { action in
					if let caps = dispatcher.caps(for: action).first {
						SequenceCaps(caps: caps)
							.help(dispatcher.title(for: action) ?? "")
					}
				}
			} else {
				ForEach(dispatcher.completions.filter { !$0.isInItemMenu }, id: \.self) { completion in
					HStack(spacing: 3) {
						SequenceCaps(caps: completion.sequence.map(\.capText), typed: dispatcher.pending.count)
						Text(completion.label)
							.font(Tokens.Typography.caption.font)
							.foregroundStyle(Tokens.Colors.inkSecondary)
							.lineLimit(1)
					}
				}
			}
		}
		.transition(CapTransition.transition(for: motion))
	}

	private var motion: ShortcutCapMotion {
		ShortcutCapMotion(setting: app.settings?.shortcutCaps ?? .animated, reduceMotion: reduceMotion)
	}
}

/// The grow transition of the shortcut caps and its two fallbacks.
enum CapTransition {
	static func transition(for motion: ShortcutCapMotion) -> AnyTransition {
		switch motion {
		case let .grow(duration):
			AnyTransition.scale(scale: 0.6, anchor: .leading).combined(with: .opacity).animation(.easeOut(duration: duration))
		case let .crossfade(duration):
			AnyTransition.opacity.animation(.easeOut(duration: duration))
		case .instant:
			AnyTransition.identity
		}
	}
}

/// The selected item's more-actions menu, opened by `\` or by a pending
/// stroke that completes inside it: the matching rows light, the rest dim.
/// A signal card's menu lists the signal's rows, a task card's the task's.
struct ItemMenuPanel: View {
	let app: AppModel
	var isForSignal = false

	var body: some View {
		let dispatcher = app.dispatcher
		let rows = dispatcher.keyTable.filter { $0.isInItemMenu && $0.action.isSignalAction == isForSignal }
		let lit = Set(dispatcher.completions.filter(\.isInItemMenu).map(\.action))
		let isPending = !dispatcher.completions.isEmpty
		VStack(alignment: .leading, spacing: 2) {
			Text(isPending ? "Opened by \(dispatcher.pendingText)" : "More actions")
				.font(Tokens.Typography.eyebrow.font)
				.textCase(.uppercase)
				.foregroundStyle(Tokens.Colors.inkMuted)
				.padding(EdgeInsets(top: 4, leading: 8, bottom: 2, trailing: 8))
			ForEach(rows, id: \.action) { row in
				let isLit = lit.contains(row.action)
				Button {
					app.keyboard.closeItemMenu()
					app.perform(row.action)
				} label: {
					HStack(spacing: Tokens.Spacing.space8) {
						Text(row.label)
							.frame(maxWidth: .infinity, alignment: .leading)
						// Reply in the mail app is `gx`, the open-the-link key, on a signal.
						if let caps = row.caps.first ?? (row.action == .signalReply ? dispatcher.caps(for: .openLink).first : nil) {
							SequenceCaps(caps: caps, typed: isLit ? dispatcher.pending.count : 0)
						}
					}
					.padding(.horizontal, 8)
					.padding(.vertical, 5)
					.foregroundStyle(isPending && !isLit ? Tokens.Colors.inkMuted : Tokens.Colors.ink)
					.background(isLit ? Tokens.Colors.sunriseSoft : .clear, in: RoundedRectangle(cornerRadius: 5))
					.contentShape(Rectangle())
				}
				.buttonStyle(.plain)
			}
		}
		.font(Tokens.Typography.callout.font)
		.padding(5)
		.frame(width: 260)
		.background(Tokens.Colors.surfaceRaised, in: RoundedRectangle(cornerRadius: Tokens.Radius.radiusLg))
		.overlay(RoundedRectangle(cornerRadius: Tokens.Radius.radiusLg).strokeBorder(Tokens.Colors.line))
		.shadow(color: .black.opacity(0.16), radius: 12, y: 8)
		.accessibilityElement(children: .contain)
		.accessibilityLabel("More actions")
	}
}

/// The change keys' editor: one field for the text, estimate, due day or
/// scheduled day of the selected task. ⏎ saves through the register, Esc cancels.
struct ChangeFieldEditor: View {
	let app: AppModel
	let request: TaskChangeRequest
	@State private var value = ""
	@FocusState private var isFocused: Bool

	var body: some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space8) {
			Text(request.title)
				.font(Tokens.Typography.headline.font)
			VimTextField(placeholder, text: $value, focusesOnAppear: true) {
				// Esc in Normal mode cancels, as Esc does under Standard.
				app.keyboard.cancelChange()
				return true
			}
				.textFieldStyle(.plain)
				.focused($isFocused)
				.padding(.horizontal, 10)
				.frame(height: 30)
				.background(Tokens.Colors.surface, in: RoundedRectangle(cornerRadius: Tokens.Radius.radiusMd))
				.overlay(RoundedRectangle(cornerRadius: Tokens.Radius.radiusMd).strokeBorder(Tokens.Colors.focus))
				.vimModeLineBelow()
				.onSubmit { app.keyboard.submitChange(value) }
				.onExitCommand { app.keyboard.cancelChange() }
			Text("\(request.taskId) · ⏎ saves, Esc cancels, u undoes")
				.font(Tokens.Typography.caption.font)
				.foregroundStyle(Tokens.Colors.inkMuted)
		}
		.padding(Tokens.Spacing.space12)
		.frame(width: 360)
		.background(Tokens.Colors.surfaceRaised, in: RoundedRectangle(cornerRadius: Tokens.Radius.radiusLg))
		.shadow(color: .black.opacity(0.18), radius: 16, y: 10)
		.onAppear {
			value = request.initialValue
			// `c` changes in Vim: the editor types at once, as `cc` does.
			VimFieldRouter.shared.startNextFieldInInsert()
			isFocused = true
		}
		.onChange(of: isFocused) { app.setTextField("change", isFocused: isFocused) }
	}

	private var placeholder: String {
		switch request.field {
		case .text: "Task text"
		case .estimate: "45m, 2h; empty clears it"
		case .due, .scheduled: "today, tomorrow or 2026-10-30; empty clears it"
		}
	}
}

/// The register's last word: what it did, or why it could not.
struct RegisterNote: View {
	let register: PlanRegister

	var body: some View {
		if let message = register.message {
			Text(message)
				.font(Tokens.Typography.caption.font)
				.foregroundStyle(Tokens.Colors.inkSecondary)
				.padding(.horizontal, Tokens.Spacing.space8)
				.padding(.vertical, 4)
				.background(Tokens.Colors.surfaceRaised, in: RoundedRectangle(cornerRadius: 6))
				.overlay(RoundedRectangle(cornerRadius: 6).strokeBorder(Tokens.Colors.line))
				.padding(Tokens.Spacing.space12)
				.accessibilityAddTraits(.updatesFrequently)
		}
	}
}
