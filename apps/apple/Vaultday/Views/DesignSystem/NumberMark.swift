import AppKit
import SwiftUI

/// A number centred in a fixed square or circle: the workspace badge, the
/// step mark and the badge inside a scope chip. Digits are tabular, so one
/// and two digits sit in the middle alike, and the line box is shifted so
/// the digits' cap height, not the font's ascender and descender, is what
/// centres in the box.
struct NumberMark: View {
	enum Outline { case square, circle }

	let number: Int
	let metrics: NumberMarkMetrics
	var outline: Outline = .square
	var foreground: Color
	var fill: Color
	/// A 1 pt edge drawn inside the box, as on an upcoming step's mark.
	var stroke: Color?

	var body: some View {
		let text = "\(number)"
		let fontSize = metrics.fontSize(digits: text.count)
		Text(text)
			.font(.custom(metrics.family, size: fontSize).weight(.bold).monospacedDigit())
			.tracking(metrics.tracking(digits: text.count))
			.foregroundStyle(foreground)
			.lineLimit(1)
			.fixedSize()
			.offset(y: NumberMarkMetrics.capCentringOffset(for: metrics.nsFont(size: fontSize)))
			.frame(width: metrics.box, height: metrics.box)
			.background(fill, in: backgroundShape)
			.overlay {
				if let stroke { backgroundShape.strokeBorder(stroke, lineWidth: 1) }
			}
	}

	/// A circle is the square with its corners rounded all the way.
	private var backgroundShape: RoundedRectangle {
		RoundedRectangle(cornerRadius: outline == .circle ? metrics.box / 2 : Tokens.Radius.radiusXs)
	}
}

/// The box and type of a number mark, from the WorkspaceBadge and StepList
/// guidelines: a badge's single digit is 10 pt in a 14 pt square and 11 pt in
/// a 16 pt one; a step mark's is 11 pt in an 18 pt circle. Two digits drop a
/// point and close up a little so they keep a margin inside the box.
struct NumberMarkMetrics: Equatable, Sendable {
	let box: CGFloat
	let singleDigitSize: CGFloat
	var family = Tokens.Typography.caption.family

	/// The workspace badge in lists and the sidebar.
	static let badge = NumberMarkMetrics(box: 14, singleDigitSize: 10)
	/// The workspace badge in a card's icon box.
	static let badgeLarge = NumberMarkMetrics(box: 16, singleDigitSize: 11)
	/// The onboarding step mark.
	static let step = NumberMarkMetrics(box: 18, singleDigitSize: 11)

	func fontSize(digits: Int) -> CGFloat {
		digits > 1 ? singleDigitSize - 1 : singleDigitSize
	}

	func tracking(digits: Int) -> CGFloat {
		digits > 1 ? -0.4 : 0
	}

	/// How far down the line box moves so the cap height's middle lands on
	/// the box's middle. A line box is ascender plus descender (plus leading)
	/// tall, and digits stand on the baseline up to the cap height.
	static func capCentringOffset(ascender: CGFloat, descender: CGFloat, leading: CGFloat, capHeight: CGFloat) -> CGFloat {
		let depth = abs(descender)
		let lineHeight = ascender + depth + leading
		let digitsMiddleFromTop = leading / 2 + ascender - capHeight / 2
		return lineHeight / 2 - digitsMiddleFromTop
	}

	static func capCentringOffset(for font: NSFont) -> CGFloat {
		capCentringOffset(ascender: font.ascender, descender: font.descender, leading: font.leading, capHeight: font.capHeight)
	}

	/// The AppKit face SwiftUI draws the number with, for its metrics.
	func nsFont(size: CGFloat) -> NSFont {
		let descriptor = NSFontDescriptor(fontAttributes: [
			.family: family,
			.traits: [NSFontDescriptor.TraitKey.weight: NSFont.Weight.bold],
		])
		return NSFont(descriptor: descriptor, size: size) ?? .systemFont(ofSize: size, weight: .bold)
	}
}
