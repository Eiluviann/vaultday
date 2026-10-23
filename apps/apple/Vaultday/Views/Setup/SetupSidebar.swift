import SwiftUI
import VaultdayCore

/// The 240 px step rail: the brand row, the StepList with the current step
/// raised, and the keyboard scheme at the foot with the motions popover.
struct SetupSidebar: View {
	let onboarding: OnboardingModel
	@Binding var scheme: KeyboardSchemeSetting
	@State private var isShowingMotions = false

	var body: some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space16) {
			brandRow
			StepList(onboarding: onboarding)
			Spacer(minLength: Tokens.Spacing.space16)
			HStack(spacing: Tokens.Spacing.space6) {
				SegmentedChoice(
					label: "Keyboard scheme",
					options: [(KeyboardSchemeSetting.standard, "Standard"), (.vimMotions, "Vim motions")],
					selection: $scheme,
					ground: .sunken,
					fills: true,
				)
				Button {
					isShowingMotions.toggle()
				} label: {
					SetupLabel(title: "", icon: "info.circle")
				}
				.buttonStyle(SetupButtonStyle(kind: .secondary, isSmall: true))
				.accessibilityLabel("Preview the vim motions")
				.help("Preview the motions")
				.popover(isPresented: $isShowingMotions, arrowEdge: .trailing) {
					MotionsPopover(scheme: $scheme)
				}
			}
			.padding(.horizontal, 6)
			Text("These settings can be changed later in Settings")
				.textStyle(Tokens.Typography.caption)
				.foregroundStyle(Tokens.Colors.inkMuted)
				.padding(.horizontal, 6)
		}
		.padding(.vertical, Tokens.Spacing.space24)
		.padding(.horizontal, Tokens.Spacing.space16)
		.frame(width: SetupLayout.sidebarWidth)
		.frame(maxHeight: .infinity, alignment: .top)
		.background(Tokens.Colors.surfaceSidebar)
		.overlay(alignment: .trailing) { Rectangle().fill(Tokens.Colors.line).frame(width: 1) }
	}

	private var brandRow: some View {
		HStack(spacing: Tokens.Spacing.space8) {
			Image("InAppPorthole")
				.resizable()
				.frame(width: 34, height: 34)
				.accessibilityHidden(true)
			VStack(alignment: .leading, spacing: 0) {
				Text("Vaultday").textStyle(Tokens.Typography.heading).foregroundStyle(Tokens.Colors.ink)
				Text("First run on this Mac").textStyle(Tokens.Typography.caption).foregroundStyle(Tokens.Colors.inkMuted)
			}
		}
		.padding(.horizontal, 6)
	}
}

/// The twelve steps: steps done carry a slate check and are links back, the
/// shown one is raised with a sunrise number, the rest a muted outline.
struct StepList: View {
	let onboarding: OnboardingModel

	var body: some View {
		VStack(spacing: Tokens.Spacing.space2) {
			ForEach(SetupStep.allCases) { step in
				let mark = onboarding.mark(of: step)
				Button {
					onboarding.jump(to: step)
				} label: {
					HStack(spacing: Tokens.Spacing.space8) {
						stepMark(step, mark: mark)
						Text(step.title)
							.textStyle(Tokens.Typography.body)
							.fontWeight(mark == .current ? .bold : .medium)
						Spacer(minLength: 0)
					}
					.foregroundStyle(titleColor(mark))
					.padding(.horizontal, Tokens.Spacing.space8)
					.padding(.vertical, 5)
					.background {
						if mark == .current {
							RoundedRectangle(cornerRadius: Tokens.Radius.radiusMd)
								.fill(Tokens.Colors.surfaceRaised)
								.raisedShadow()
						}
					}
					.contentShape(Rectangle())
				}
				.buttonStyle(.plain)
				.disabled(mark == .upcoming)
				.accessibilityLabel("\(step.title), step \(step.rawValue)")
				.accessibilityAddTraits(mark == .current ? .isSelected : [])
			}
		}
		.accessibilityElement(children: .contain)
		.accessibilityLabel("Setup steps")
	}

	@ViewBuilder
	private func stepMark(_ step: SetupStep, mark: StepMark) -> some View {
		if mark == .done {
			Image(systemName: "checkmark")
				.iconGlyph(.tag, weight: .bold)
				.foregroundStyle(Tokens.Colors.onPrimary)
				.frame(width: NumberMarkMetrics.step.box, height: NumberMarkMetrics.step.box)
				.background(Tokens.Colors.primary, in: Circle())
				.accessibilityLabel("Done")
		} else {
			NumberMark(
				number: step.rawValue,
				metrics: .step,
				outline: .circle,
				foreground: markForeground(mark),
				fill: markFill(mark),
				stroke: mark == .upcoming ? Tokens.Colors.lineControl : nil,
			)
		}
	}

	private func titleColor(_ mark: StepMark) -> Color {
		switch mark {
		case .current: Tokens.Colors.ink
		case .done: Tokens.Colors.inkSecondary
		case .upcoming: Tokens.Colors.inkMuted
		}
	}

	private func markFill(_ mark: StepMark) -> Color {
		switch mark {
		case .current: Tokens.Colors.sunrise
		case .done: Tokens.Colors.primary
		case .upcoming: .clear
		}
	}

	private func markForeground(_ mark: StepMark) -> Color {
		switch mark {
		case .current: Tokens.Colors.onSunrise
		case .done: Tokens.Colors.onPrimary
		case .upcoming: Tokens.Colors.inkMuted
		}
	}
}

/// The Vim motions preview: what the keys do in the setup and everywhere in Vaultday.
struct MotionsPopover: View {
	@Binding var scheme: KeyboardSchemeSetting

	var body: some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space12) {
			SegmentedChoice(
				label: "Keyboard scheme",
				options: [(KeyboardSchemeSetting.standard, "Standard"), (.vimMotions, "Vim motions")],
				selection: $scheme,
				fills: true,
			)
			HStack(spacing: Tokens.Spacing.space8) {
				Image(systemName: "terminal")
				Text("Vim motions").textStyle(Tokens.Typography.title)
			}
			.foregroundStyle(Tokens.Colors.ink)
			Text("One shortcut set everywhere: single keys act on the selected item, so the mouse is optional from here on")
				.textStyle(Tokens.Typography.callout)
				.foregroundStyle(Tokens.Colors.inkSecondary)
				.fixedSize(horizontal: false, vertical: true)
			group("In this setup", [
				("Move between items", ["j", "k"]),
				("Select or toggle", ["Space"]),
				("Continue · Back", ["⏎", "⌫"]),
			])
			group("Everywhere in Vaultday", [
				("Move · first, last · half page", ["j", "k", "gg", "G", "^d", "^u"]),
				("Columns · groups · panes", ["h", "l", "}", "{", "]", "["]),
				("Act on the selected task: timer, done, open, peek", ["Space", "x", "⏎", "K"]),
				("Change · cut · yank · paste", ["cc", "dd", "yy", "p"]),
				("Search · command palette", ["/", ":"]),
				("Type into a field · leave it", ["i", "Esc"]),
			])
			Text("Standard keeps the usual Mac keys: arrows, Tab, Return and ⌘ shortcuts")
				.textStyle(Tokens.Typography.caption)
				.foregroundStyle(Tokens.Colors.inkMuted)
		}
		.padding(Tokens.Spacing.space16)
		.frame(width: 440)
	}

	private func group(_ title: String, _ rows: [(String, [String])]) -> some View {
		VStack(alignment: .leading, spacing: Tokens.Spacing.space6) {
			Eyebrow(text: title)
			ForEach(rows.indices, id: \.self) { index in
				HStack(alignment: .firstTextBaseline) {
					Text(rows[index].0)
						.textStyle(Tokens.Typography.callout)
						.foregroundStyle(Tokens.Colors.ink)
					Spacer(minLength: Tokens.Spacing.space8)
					HStack(spacing: Tokens.Spacing.space4) {
						ForEach(rows[index].1, id: \.self) { KbdCap(text: $0) }
					}
				}
			}
		}
	}
}
