import SwiftUI

/// Marks a drop target while a drag hovers over it and never at rest: the
/// design shows a dashed border only while something is being dragged in.
/// A plain modifier, for the design system to restyle.
struct SetupDropZone: ViewModifier {
	let isTargeted: Bool

	func body(content: Content) -> some View {
		content.overlay {
			if isTargeted {
				RoundedRectangle(cornerRadius: Tokens.Radius.radiusLg)
					.strokeBorder(Tokens.Colors.primary, style: StrokeStyle(lineWidth: 1.5, dash: [5, 4]))
					.allowsHitTesting(false)
			}
		}
	}
}

extension View {
	/// The drop indicator of the setup steps' drag and drop.
	func setupDropZone(isTargeted: Bool) -> some View {
		modifier(SetupDropZone(isTargeted: isTargeted))
	}
}
