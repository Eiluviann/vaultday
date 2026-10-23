/** Swift support types the generated constants depend on, emitted after the `Tokens` enum. */
export const SWIFT_SUPPORT_TYPES = `/// An sRGB color given as 0...255 channels and a 0...1 alpha.
struct RGBA: Sendable {
	let red: Double
	let green: Double
	let blue: Double
	let alpha: Double

	init(_ red: Int, _ green: Int, _ blue: Int, _ alpha: Double) {
		self.red = Double(red) / 255
		self.green = Double(green) / 255
		self.blue = Double(blue) / 255
		self.alpha = alpha
	}

	var nsColor: NSColor {
		NSColor(srgbRed: red, green: green, blue: blue, alpha: alpha)
	}
}

extension Color {
	/// A color that follows the appearance of the view it is drawn in.
	init(light: RGBA, dark: RGBA) {
		self.init(nsColor: NSColor(name: nil) { appearance in
			let isDarkAppearance = appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
			return isDarkAppearance ? dark.nsColor : light.nsColor
		})
	}
}

/// A type style: family, size and line height in points, weight and tracking in em.
struct TextStyle: Sendable {
	let family: String
	let size: CGFloat
	let lineHeight: CGFloat
	let weight: Font.Weight
	let letterSpacingEm: CGFloat

	var font: Font { .custom(family, size: size).weight(weight) }
	var lineSpacing: CGFloat { max(0, lineHeight - size) }
	var tracking: CGFloat { letterSpacingEm * size }
}

/// One layer of a drop shadow or ring, as in a CSS box-shadow.
struct ShadowLayer: Sendable {
	let x: CGFloat
	let y: CGFloat
	let blur: CGFloat
	let spread: CGFloat
	let color: RGBA
}

/// A shadow per appearance.
struct ShadowStyle: Sendable {
	let light: [ShadowLayer]
	let dark: [ShadowLayer]
}
`;
