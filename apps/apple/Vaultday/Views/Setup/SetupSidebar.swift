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
				.font(Tokens.Typography.caption.font)
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
				Text("Vaultday").font(Tokens.Typography.heading.font).foregroundStyle(Tokens.Colors.ink)
				Text("First run on this Mac").font(Tokens.Typography.caption.font).foregroundStyle(Tokens.Colors.inkMuted)
			}
		}
		.padding(.horizontal, 6)
	}
}

/// The twelve steps: steps reached are links back, the shown one is raised with a sunrise mark.
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
						Text("\(step.rawValue)")
							.font(Tokens.Typography.caption.font.weight(.bold))
							.frame(width: 18, height: 18)
							.foregroundStyle(markForeground(mark))
							.background(markFill(mark), in: Circle())
							.overlay(Circle().strokeBorder(mark == .upcoming ? Tokens.Colors.lineControl : .clear, lineWidth: 1))
						Text(step.title)
							.font(Tokens.Typography.body.font.weight(mark == .current ? .bold : .medium))
						Spacer(minLength: 0)
					}
					.foregroundStyle(titleColor(mark))
					.padding(.horizontal, Tokens.Spacing.space8)
					.padding(.vertical, 5)
					.background {
						if mark == .current {
							RoundedRectangle(cornerRadius: Tokens.Radius.radiusMd)
								.fill(Tokens.Colors.surfaceRaised)
								.shadow(color: .black.opacity(0.12), radius: 1, y: 1)
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
				Text("Vim motions").font(Tokens.Typography.title.font)
			}
			.foregroundStyle(Tokens.Colors.ink)
			Text("One shortcut set everywhere: single keys act on the selected item, so the mouse is optional from here on")
				.font(Tokens.Typography.callout.font)
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
				.font(Tokens.Typography.caption.font)
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
						.font(Tokens.Typography.callout.font)
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
