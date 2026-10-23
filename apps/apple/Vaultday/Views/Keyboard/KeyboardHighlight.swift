import SwiftUI

/// The keyboard highlight on a row or control the keys walk (a sidebar
/// row, a header control, a setup option): the focus ring, 2 px, outside
/// the item. One place, so the look of focus changes in one place.
struct KeyboardHighlight: ViewModifier {
	let isOn: Bool
	var cornerRadius: CGFloat = Tokens.Radius.radiusMd

	func body(content: Content) -> some View {
		content.overlay {
			if isOn {
				RoundedRectangle(cornerRadius: cornerRadius + 2)
					.strokeBorder(Tokens.Colors.focus, lineWidth: 2)
					.padding(-3)
					.allowsHitTesting(false)
			}
		}
	}
}

extension View {
	/// The focus ring while the keys are on this item.
	func keyboardHighlight(_ isOn: Bool, cornerRadius: CGFloat = Tokens.Radius.radiusMd) -> some View {
		modifier(KeyboardHighlight(isOn: isOn, cornerRadius: cornerRadius))
	}
}
