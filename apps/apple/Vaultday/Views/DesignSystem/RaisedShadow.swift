import SwiftUI

/// The design system's `shadow-raised` in the appearance the view is drawn
/// in: the lift of a chosen segment and of the current setup step. The
/// token's blur is a CSS blur, twice SwiftUI's shadow radius.
struct RaisedShadow: ViewModifier {
	@Environment(\.colorScheme) private var colorScheme

	func body(content: Content) -> some View {
		let style = Tokens.Shadows.shadowRaised
		if let layer = (colorScheme == .dark ? style.dark : style.light).first {
			content.shadow(color: Color(nsColor: layer.color.nsColor), radius: layer.blur / 2, x: layer.x, y: layer.y)
		} else {
			content
		}
	}
}

extension View {
	/// Lifts what is chosen with `shadow-raised`, light or dark.
	func raisedShadow() -> some View {
		modifier(RaisedShadow())
	}
}
