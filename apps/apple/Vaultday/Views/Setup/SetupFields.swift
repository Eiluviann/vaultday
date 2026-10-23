import SwiftUI
import VaultdayCore

// Small controls the later steps and the Settings window share: a menu, a
// time of day with an optional weekday, and a switch.

/// A pop-up menu in the design's select: 26 px, bordered, chevron right.
struct SetupMenu<Value: Hashable>: View {
	let label: String
	let options: [(value: Value, title: String)]
	@Binding var selection: Value
	var width: CGFloat?

	var body: some View {
		Picker(label, selection: $selection) {
			ForEach(options.indices, id: \.self) { index in
				Text(options[index].title).tag(options[index].value)
			}
		}
		.labelsHidden()
		.pickerStyle(.menu)
		.frame(width: width)
		.accessibilityLabel(label)
	}
}

/// A switch with its label read aloud but not drawn; the card beside it names it.
struct SetupSwitch: View {
	let label: String
	@Binding var isOn: Bool

	var body: some View {
		Toggle(label, isOn: $isOn)
			.labelsHidden()
			.toggleStyle(.switch)
			.controlSize(.small)
	}
}

/// A time of day in `HH:MM`, with a weekday menu when the ritual runs on one
/// day a week. The text is checked when it is committed (Return, or leaving
/// the field); a time that is not on the clock keeps the field red and writes
/// nothing.
struct DayTimeField: View {
	let label: String
	let time: DayTime
	var day: WeekdayChoice?
	var issue: String?
	let setTime: (String) -> Bool
	var setDay: (WeekdayChoice) -> Void = { _ in }

	@State private var text = ""
	@State private var isRefused = false
	@FocusState private var isFocused: Bool

	var body: some View {
		HStack(spacing: Tokens.Spacing.space6) {
			if let day {
				SetupMenu(label: "\(label) day", options: WeekdayChoice.allCases.map { ($0, $0.title) }, selection: Binding(get: { day }, set: setDay), width: 74)
			}
			TextField(label, text: $text)
				.textFieldStyle(.plain)
				.focused($isFocused)
				.multilineTextAlignment(.trailing)
				.font(Tokens.Typography.callout.font.monospacedDigit())
				.frame(width: 58, height: Tokens.Size.sizeControlSm + 4)
				.padding(.horizontal, Tokens.Spacing.space6)
				.background(Tokens.Colors.surfaceRaised, in: RoundedRectangle(cornerRadius: Tokens.Radius.radiusSm))
				.overlay(RoundedRectangle(cornerRadius: Tokens.Radius.radiusSm).strokeBorder(isRefused ? Tokens.Colors.caution : Tokens.Colors.lineControl, lineWidth: 1))
				.onSubmit(commit)
				.onChange(of: isFocused) { _, focused in
					if !focused { commit() }
				}
				.onChange(of: time) { _, new in
					if !isFocused { text = new.text; isRefused = false }
				}
				.accessibilityLabel(label)
				.accessibilityHint(issue ?? "A time like 08:45")
		}
		.onAppear { text = time.text }
	}

	private func commit() {
		guard text != time.text else { isRefused = false; return }
		if setTime(text) {
			isRefused = false
		} else {
			isRefused = true
		}
	}
}
