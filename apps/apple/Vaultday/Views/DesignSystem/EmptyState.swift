import SwiftUI

/// An empty state: a muted glyph over one muted line, on the pane's own
/// ground with no border. A dashed edge means "drop here", so it is drawn
/// only by `dropIndicator(isActive:)` while something is being dragged or
/// reordered, never around an empty state.
struct EmptyState: View {
	let icon: String
	let text: String
	/// A second, quieter line: what fills the place, or the next step.
	var detail: String?
	/// The glyph's tint; the muted ink unless the state is good news.
	var tint: Color = Tokens.Colors.inkMuted

	var body: some View {
		VStack(spacing: Tokens.Spacing.space6) {
			Image(systemName: icon)
				.iconBox(.menu)
				.foregroundStyle(tint)
			Text(text)
				.textStyle(Tokens.Typography.body)
				.foregroundStyle(Tokens.Colors.inkMuted)
				.multilineTextAlignment(.center)
			if let detail {
				Text(detail)
					.textStyle(Tokens.Typography.callout)
					.foregroundStyle(Tokens.Colors.inkMuted)
					.multilineTextAlignment(.center)
			}
		}
		.padding(.vertical, Tokens.Spacing.space16)
		.padding(.horizontal, Tokens.Spacing.space12)
		.frame(maxWidth: .infinity)
		.accessibilityElement(children: .combine)
	}
}

/// The one dashed edge in the app: where a dragged item can land, shown
/// while it is over the place (or while a list is being reordered).
struct DropIndicator: ViewModifier {
	let isActive: Bool
	var cornerRadius: CGFloat = Tokens.Radius.radiusLg

	func body(content: Content) -> some View {
		content.overlay {
			if isActive {
				RoundedRectangle(cornerRadius: cornerRadius)
					.strokeBorder(Tokens.Colors.focus, style: DropIndicator.stroke)
					.allowsHitTesting(false)
			}
		}
	}

	/// The drop edge: 1.5 pt in the focus color, 5 on and 4 off.
	static let stroke = StrokeStyle(lineWidth: 1.5, dash: [5, 4])
}

extension View {
	/// Draws the dashed drop edge while `isActive`, and nothing otherwise.
	func dropIndicator(isActive: Bool, cornerRadius: CGFloat = Tokens.Radius.radiusLg) -> some View {
		modifier(DropIndicator(isActive: isActive, cornerRadius: cornerRadius))
	}
}
