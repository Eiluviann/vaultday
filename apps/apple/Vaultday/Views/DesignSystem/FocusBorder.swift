import SwiftUI

/// Keyboard focus, drawn: a thick border in the focus color inside the
/// focused pane or row, never a tinted ground. Visual only; which pane or row
/// holds the focus is the keyboard model's business.
struct FocusBorder: ViewModifier {
	let isFocused: Bool
	var cornerRadius: CGFloat = 0

	func body(content: Content) -> some View {
		content.overlay {
			RoundedRectangle(cornerRadius: cornerRadius)
				.strokeBorder(Tokens.Colors.focus, lineWidth: FocusBorder.width)
				.opacity(isFocused ? 1 : 0)
				.animation(.easeOut(duration: Tokens.Motion.motionWash), value: isFocused)
				.allowsHitTesting(false)
		}
	}

	/// Thick enough to read at a glance across a pane, as the 2 pt focus ring is on a control.
	static let width: CGFloat = 2
}

/// A pane's ground: its own color always, and the focus border inside its
/// edges while the pane has keyboard focus. The ground never changes with focus.
struct PaneGround: ViewModifier {
	let ground: Color
	let isFocused: Bool

	func body(content: Content) -> some View {
		content
			.background(ground)
			.focusBorder(isFocused)
	}
}

extension View {
	/// Shows keyboard focus with the thick focus border; `cornerRadius` follows the shape it frames.
	func focusBorder(_ isFocused: Bool, cornerRadius: CGFloat = 0) -> some View {
		modifier(FocusBorder(isFocused: isFocused, cornerRadius: cornerRadius))
	}

	/// The pane's ground and, while it has keyboard focus, the focus border.
	func paneGround(_ ground: Color, isFocused: Bool) -> some View {
		modifier(PaneGround(ground: ground, isFocused: isFocused))
	}
}
