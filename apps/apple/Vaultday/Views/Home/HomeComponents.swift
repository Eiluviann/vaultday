import SwiftUI
import VaultdayCore

/// An eyebrow above a list: 11 px bold, uppercase by style.
struct HomeEyebrow: View {
	let text: String

	var body: some View {
		Text(text.uppercased())
			.font(Tokens.Typography.eyebrow.font)
			.tracking(Tokens.Typography.eyebrow.tracking)
			.foregroundStyle(Tokens.Colors.inkMuted)
	}
}

/// A split pill of the day totals: the name on the control fill, the value on raised.
struct TotalPill: View {
	let name: String
	let value: String
	var isCaution = false
	var help = ""

	var body: some View {
		HStack(spacing: 0) {
			Text(name)
				.foregroundStyle(Tokens.Colors.inkSecondary)
				.padding(.horizontal, 9)
				.padding(.vertical, 5)
				.background(Tokens.Colors.fillControl)
			Text(value)
				.foregroundStyle(isCaution ? Tokens.Colors.caution : Tokens.Colors.ink)
				.monospacedDigit()
				.padding(.horizontal, 9)
				.padding(.vertical, 5)
				.background(Tokens.Colors.surfaceRaised)
		}
		.font(Tokens.Typography.callout.font.weight(.semibold))
		.clipShape(Capsule())
		.overlay(Capsule().strokeBorder(Tokens.Colors.line))
		.help(help)
		.accessibilityElement(children: .combine)
	}
}

/// A mono pill: a ticket number or an id.
struct MonoChip: View {
	let text: String
	var isRounded = true

	var body: some View {
		Text(text)
			.font(.custom(Tokens.Typography.mono.family, size: 11))
			.foregroundStyle(Tokens.Colors.inkMuted)
			.lineLimit(1)
			.padding(.horizontal, isRounded ? 8 : 5)
			.padding(.vertical, isRounded ? 3 : 1)
			.background(Tokens.Colors.fillControl, in: RoundedRectangle(cornerRadius: isRounded ? Tokens.Radius.radiusPill : Tokens.Radius.radiusXs))
	}
}

/// A caution pill, such as "waiting 6d".
struct CautionChip: View {
	let text: String

	var body: some View {
		Text(text)
			.font(Tokens.Typography.caption.font.weight(.semibold))
			.foregroundStyle(Tokens.Colors.caution)
			.lineLimit(1)
			.padding(.horizontal, 8)
			.padding(.vertical, 3)
			.background(Tokens.Colors.cautionSoft, in: Capsule())
	}
}

/// The 26 × 24 bordered icon button of the Today header (‹ › wand =).
struct HeaderIconButton: View {
	let symbol: String?
	var text: String?
	let label: String
	let help: String
	let action: () -> Void

	var body: some View {
		Button(action: action) {
			Group {
				if let symbol {
					Image(systemName: symbol).font(.system(size: 11))
				} else {
					Text(text ?? "")
				}
			}
			.frame(width: 26, height: 24)
			.foregroundStyle(Tokens.Colors.ink)
			.background(Tokens.Colors.surfaceRaised, in: RoundedRectangle(cornerRadius: Tokens.Radius.radiusSm))
			.overlay(RoundedRectangle(cornerRadius: Tokens.Radius.radiusSm).strokeBorder(Tokens.Colors.lineButton))
		}
		.buttonStyle(.plain)
		.help(help)
		.accessibilityLabel(label)
	}
}

/// The small bordered square button inside cards and subtask rows (… ▶ ❙❙).
struct CardIconButton: View {
	let symbol: String
	let label: String
	var help = ""
	var size: CGFloat = 12
	let action: () -> Void

	var body: some View {
		Button(action: action) {
			Image(systemName: symbol)
				.font(.system(size: size - 3))
				.iconReplace(value: symbol)
				.frame(minWidth: 24, minHeight: 20)
				.foregroundStyle(Tokens.Colors.inkSecondary)
				.background(Tokens.Colors.surfaceRaised, in: RoundedRectangle(cornerRadius: Tokens.Radius.radiusSm))
				.overlay(RoundedRectangle(cornerRadius: Tokens.Radius.radiusSm).strokeBorder(Tokens.Colors.lineButton))
		}
		.buttonStyle(.plain)
		.help(help.isEmpty ? label : help)
		.accessibilityLabel(label)
	}
}

/// A key cap named by the keymap: nothing when the scheme leaves the action unbound.
struct ActionCap: View {
	let app: AppModel
	let action: KeyAction
	/// The fill the cap sits on, when it is a colored button rather than the page.
	var onFill: Color?

	var body: some View {
		if let label = app.keyLabel(action) {
			Text(label)
				.font(Tokens.Typography.kbd.font)
				.foregroundStyle(onFill ?? Tokens.Colors.inkSecondary)
				.padding(.horizontal, 4)
				.padding(.vertical, 2)
				.background(onFill?.opacity(0.28) ?? Tokens.Colors.fillControl, in: RoundedRectangle(cornerRadius: Tokens.Radius.radiusXs))
		}
	}
}

/// A card that lifts on hover and while selected.
struct LiftModifier: ViewModifier {
	let isLifted: Bool

	func body(content: Content) -> some View {
		content
			.shadow(color: .black.opacity(isLifted ? 0.10 : 0), radius: isLifted ? 8 : 0, y: isLifted ? 6 : 0)
			.shadow(color: .black.opacity(isLifted ? 0.06 : 0), radius: isLifted ? 1 : 0, y: isLifted ? 1 : 0)
			.offset(y: isLifted ? -1 : 0)
			.animation(.easeOut(duration: Tokens.Motion.motionLift), value: isLifted)
	}
}

extension View {
	func lift(_ isLifted: Bool) -> some View {
		modifier(LiftModifier(isLifted: isLifted))
	}
}

/// A source or type mark's colour from its token.
extension MarkTone {
	var color: Color {
		switch self {
		case .inkSecondary: Tokens.Colors.inkSecondary
		case .inkMuted: Tokens.Colors.inkMuted
		case .info: Tokens.Colors.info
		case .positive: Tokens.Colors.positive
		case .caution: Tokens.Colors.caution
		case .ai: Tokens.Colors.ai
		}
	}
}

/// A source or type mark: its one icon, in its one colour.
struct MarkIcon: View {
	let mark: IconMark
	var size: CGFloat = 13

	var body: some View {
		Image(systemName: mark.symbol)
			.font(.system(size: size))
			.foregroundStyle(mark.tone.color)
			.frame(width: 16, height: 16)
			.accessibilityLabel(mark.name)
	}
}

/// Where a workspace's colour shows, and how strongly: the dot on a card in
/// All, the checkbox tint, the calendar's washes. Nowhere else.
enum WorkspaceWash {
	/// The dot beside a card's title in the All view.
	static let dotSize: CGFloat = 8
	/// A calendar event.
	static let eventOpacity = 0.12
	/// An unlocked (floating) task box on the calendar.
	static let unlockedBoxOpacity = 0.10
	/// The tracked share of a task box, and its legend swatch.
	static let trackedOpacity = 0.18
	/// The selected row of a list.
	static let selectionOpacity = 0.10
}

/// A workspace's identity color, from its sidebar entry.
extension WorkspaceEntry {
	var color: Color { WorkspaceColor.resolve(accent: accent, sphere: sphere).color }
}

extension WorkspaceGroup {
	var color: Color { WorkspaceColor.resolve(accent: accent, sphere: sphere).color }
}
