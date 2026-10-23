import SwiftUI

// The one type scale and the icon boxes. Views set text with
// `.textStyle(Tokens.Typography.body)` (or the style's `font` where only a
// font fits, as inside a `Text` concatenation) and symbols with
// `.iconBox(.row)`; no view names a point size of its own.

extension View {
	/// Sets a design-system text style: face, size and weight, tracking, and
	/// the line height as spacing between lines.
	func textStyle(_ style: TextStyle) -> some View {
		font(style.font)
			.tracking(style.tracking)
			.lineSpacing(style.lineSpacing)
	}

	/// Sets a symbol in its design-system box: a square of the box's size
	/// with the glyph drawn at 3/4 of it, centered, so wide, tall and round
	/// symbols carry the same weight and line up on one grid.
	func iconBox(_ box: IconBox, weight: Font.Weight = .regular) -> some View {
		font(.system(size: box.glyphSize, weight: weight))
			.frame(width: box.size, height: box.size)
	}

	/// Sets a symbol's glyph size for a box without the square frame: for a
	/// symbol inside a label or a `Text`, where the line sets the height.
	func iconGlyph(_ box: IconBox, weight: Font.Weight = .regular) -> some View {
		font(.system(size: box.glyphSize, weight: weight))
	}
}

/// The design system's icon boxes (Icon guideline): the box is laid out, the
/// glyph sits inside it with an eighth of padding on every side.
enum IconBox: CaseIterable {
	/// 12 pt: inside tags and small badges.
	case tag
	/// 16 pt: rows, buttons and chips.
	case row
	/// 18 pt: menu items and the card's icon box.
	case menu
	/// 20 pt: panel headers.
	case panel
	/// 32 pt: heroes and empty states.
	case hero

	var size: CGFloat {
		switch self {
		case .tag: 12
		case .row: Tokens.Size.sizeIcon
		case .menu: 18
		case .panel: 20
		case .hero: 32
		}
	}

	/// The glyph's point size: the box less an eighth on each side.
	var glyphSize: CGFloat { size * 3 / 4 }
}
