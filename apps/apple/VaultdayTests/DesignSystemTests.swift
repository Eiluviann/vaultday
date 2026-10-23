import AppKit
import SwiftUI
import XCTest

/// The design-system pieces every screen draws with: numbers centred in their
/// marks, one type scale with no point sizes of a view's own, empty states
/// without a border, the drop edge only while active, a meter empty at zero.
@MainActor
final class DesignSystemTests: XCTestCase {
	/// `apps/apple/VaultdayTests/<file>` → `apps/apple`.
	private static let appleRoot = URL(fileURLWithPath: #filePath)
		.deletingLastPathComponent()
		.deletingLastPathComponent()

	override func setUp() async throws {
		DesignFonts.registerBundledFonts(in: Bundle(for: DesignSystemTests.self))
	}

	// MARK: Fonts

	func testGeistAndGeistMonoAreBundledAndRegistered() {
		XCTAssertTrue(DesignFonts.isAvailable(family: "Geist"), "Geist is drawn, not the system fallback")
		XCTAssertTrue(DesignFonts.isAvailable(family: "Geist Mono"))
		XCTAssertEqual(Tokens.Typography.body.family, "Geist")
		XCTAssertEqual(Tokens.Typography.mono.family, "Geist Mono")
	}

	func testTheBoldWeightResolvesToGeistsBoldInstance() throws {
		let font = NumberMarkMetrics.step.nsFont(size: 11)
		XCTAssertEqual(font.familyName, "Geist")
		let weight = try XCTUnwrap((font.fontDescriptor.object(forKey: .traits) as? [NSFontDescriptor.TraitKey: Any])?[.weight] as? CGFloat)
		XCTAssertGreaterThanOrEqual(weight, NSFont.Weight.semibold.rawValue)
	}

	// MARK: Number marks

	func testSingleAndTwoDigitNumbersSitInTheMiddleOfTheirMarks() throws {
		for metrics in [NumberMarkMetrics.badge, .badgeLarge, .step] {
			for number in [1, 2, 4, 7, 9, 10, 11, 12] {
				let mark = NumberMark(number: number, metrics: metrics, foreground: .white, fill: .black)
				let ink = try XCTUnwrap(Self.inkBounds(of: mark, size: metrics.box), "\(number) drew nothing")
				let middle = metrics.box / 2
				XCTAssertEqual(ink.midX, middle, accuracy: 0.75, "\(number) in a \(metrics.box) pt mark: horizontally off")
				XCTAssertEqual(ink.midY, middle, accuracy: 0.75, "\(number) in a \(metrics.box) pt mark: vertically off")
				XCTAssertLessThanOrEqual(ink.width, metrics.box - 3, "\(number) in a \(metrics.box) pt mark: no margin left")
			}
		}
	}

	func testTheCapCentringOffsetMovesTheLineBoxSoTheCapsCentre() {
		// A line 10 up and 3 down with caps 7 tall: the caps' middle is 6.5
		// from the top of a 13 pt box whose middle is 6.5; nothing to move.
		XCTAssertEqual(NumberMarkMetrics.capCentringOffset(ascender: 10, descender: -3, leading: 0, capHeight: 7), 0, accuracy: 0.001)
		// A taller ascender lifts the caps; the box moves down by half the difference.
		XCTAssertEqual(NumberMarkMetrics.capCentringOffset(ascender: 12, descender: -3, leading: 0, capHeight: 7), -1, accuracy: 0.001)
		XCTAssertEqual(NumberMarkMetrics.capCentringOffset(ascender: 10, descender: -5, leading: 0, capHeight: 7), 1, accuracy: 0.001)
	}

	func testTwoDigitsDropAPointAndCloseUp() {
		XCTAssertEqual(NumberMarkMetrics.step.fontSize(digits: 1), 11)
		XCTAssertEqual(NumberMarkMetrics.step.fontSize(digits: 2), 10)
		XCTAssertEqual(NumberMarkMetrics.badge.fontSize(digits: 1), 10)
		XCTAssertLessThan(NumberMarkMetrics.badge.tracking(digits: 2), 0)
		XCTAssertEqual(NumberMarkMetrics.badge.tracking(digits: 1), 0)
	}

	// MARK: Type scale

	func testTheScaleIsTheDesignSystemsAndNothingIsBelowTheFloor() {
		let scale: [(String, TextStyle, CGFloat, CGFloat)] = [
			("title-large", Tokens.Typography.titleLarge, 20, 26),
			("title", Tokens.Typography.title, 17, 22),
			("headline", Tokens.Typography.headline, 14, 18),
			("heading", Tokens.Typography.heading, 13, 18),
			("body", Tokens.Typography.body, 13, 18),
			("callout", Tokens.Typography.callout, 12, 16),
			("caption", Tokens.Typography.caption, 11, 14),
			("eyebrow", Tokens.Typography.eyebrow, 11, 14),
			("mono", Tokens.Typography.mono, 12, 16),
			("kbd", Tokens.Typography.kbd, 11, 14),
		]
		for (name, style, size, lineHeight) in scale {
			XCTAssertEqual(style.size, size, name)
			XCTAssertEqual(style.lineHeight, lineHeight, name)
			XCTAssertEqual(style.lineSpacing, lineHeight - size, name)
			XCTAssertGreaterThanOrEqual(style.size, 11, "\(name) is under the 11 pt floor")
		}
	}

	func testIconBoxesDrawTheirGlyphAtThreeQuarters() {
		XCTAssertEqual(IconBox.allCases.map(\.size), [12, 16, 18, 20, 32])
		for box in IconBox.allCases {
			XCTAssertEqual(box.glyphSize, box.size * 0.75)
		}
	}

	/// One scale: a view sets text with a design-system style and a symbol
	/// with an icon box, never a point size of its own. Sizes passed in as a
	/// parameter (a checkbox's `size`) are the caller's business.
	func testNoViewNamesAPointSizeOfItsOwn() throws {
		let views = Self.appleRoot.appending(path: "Vaultday/Views", directoryHint: .isDirectory)
		let enumerator = try XCTUnwrap(FileManager.default.enumerator(at: views, includingPropertiesForKeys: nil))
		let files = enumerator.compactMap { $0 as? URL }.filter { $0.pathExtension == "swift" && !$0.path.contains("/DesignSystem/") }
		XCTAssertGreaterThan(files.count, 40)
		let literalSize = try NSRegularExpression(pattern: #"\.system\(size:\s*[0-9]"#)
		for file in files {
			let source = try String(contentsOf: file, encoding: .utf8)
			let range = NSRange(source.startIndex..., in: source)
			XCTAssertEqual(literalSize.numberOfMatches(in: source, range: range), 0, "\(file.lastPathComponent) sets a point size; use textStyle or iconBox")
			XCTAssertFalse(source.contains(".custom("), "\(file.lastPathComponent) names a face; use a Tokens.Typography style")
		}
	}

	// MARK: Empty states and the drop edge

	func testAnEmptyStateHasNoBorder() throws {
		let state = EmptyState(icon: "envelope", text: "Connect a mail account")
			.frame(width: 280, height: 120)
			.background(Color.white)
		let image = try XCTUnwrap(Self.render(state, size: CGSize(width: 280, height: 120)))
		// The outer 6 pt ring is where a card's edge would be: it stays the ground.
		XCTAssertTrue(Self.ring(of: image, inset: 6).allSatisfy { $0.isWhite }, "an empty state draws an edge")
		XCTAssertTrue(Self.pixels(of: image).contains { !$0.isWhite }, "the glyph and the line are drawn")
	}

	func testTheDropEdgeShowsOnlyWhileActive() throws {
		let size = CGSize(width: 120, height: 60)
		let idle = try XCTUnwrap(Self.render(Color.white.frame(width: size.width, height: size.height).dropIndicator(isActive: false), size: size))
		XCTAssertTrue(Self.pixels(of: idle).allSatisfy { $0.isWhite }, "a drop edge drawn while nothing is dragged")
		let active = try XCTUnwrap(Self.render(Color.white.frame(width: size.width, height: size.height).dropIndicator(isActive: true), size: size))
		XCTAssertTrue(Self.ring(of: active, inset: 3).contains { !$0.isWhite }, "no drop edge while something is over the place")
		XCTAssertEqual(DropIndicator.stroke.dash, [5, 4])
	}

	func testTheFocusBorderDrawsNothingUnfocusedAndNeverTintsTheGround() throws {
		let size = CGSize(width: 100, height: 60)
		let unfocused = try XCTUnwrap(Self.render(Color.white.frame(width: size.width, height: size.height).paneGround(.white, isFocused: false), size: size))
		XCTAssertTrue(Self.pixels(of: unfocused).allSatisfy { $0.isWhite })
		let focused = try XCTUnwrap(Self.render(Color.white.frame(width: size.width, height: size.height).paneGround(.white, isFocused: true), size: size))
		XCTAssertTrue(Self.ring(of: focused, inset: 1).allSatisfy { !$0.isWhite }, "the border runs all round")
		let middle = Self.pixels(of: focused, in: CGRect(x: 20, y: 20, width: 60, height: 20))
		XCTAssertTrue(middle.allSatisfy { $0.isWhite }, "focus tints the ground")
		XCTAssertEqual(FocusBorder.width, 2)
	}

	// MARK: Meter and title bar

	func testTheMeterFillsNothingAtZero() {
		XCTAssertEqual(ProgressTrack.fillWidth(value: 0, trackWidth: 300), 0)
		XCTAssertEqual(ProgressTrack.fillWidth(value: -0.2, trackWidth: 300), 0)
		XCTAssertEqual(ProgressTrack.fillWidth(value: .nan, trackWidth: 300), 0)
		XCTAssertEqual(ProgressTrack.fillWidth(value: 0.001, trackWidth: 300), ProgressTrack.height, "a little reads as a pill, not a dot")
		XCTAssertEqual(ProgressTrack.fillWidth(value: 0.5, trackWidth: 300), 150)
		XCTAssertEqual(ProgressTrack.fillWidth(value: 1.4, trackWidth: 300), 300)
	}

	func testTheTrafficLightsCentreInTheThirtyEightPointBand() {
		XCTAssertEqual(WindowChrome.titleBarHeight, 38)
		XCTAssertEqual(WindowChrome.titleBarHeight, SetupLayout.titleBarHeight)
		XCTAssertEqual(TrafficLightPlacer.centredOrigin(buttonHeight: 14, bandHeight: 38), 12)
		XCTAssertEqual(TrafficLightPlacer.centredOrigin(buttonHeight: 16, bandHeight: 38), 11)
	}

	func testSetupContentKeepsToTheBoardsContentWidth() {
		XCTAssertEqual(SetupLayout.contentMaxWidth, 1100 - 240 - 2 * 32)
	}

	// MARK: Rendering helpers

	private struct Pixel {
		let red: UInt8, green: UInt8, blue: UInt8, alpha: UInt8
		var isWhite: Bool { alpha < 8 || (red > 245 && green > 245 && blue > 245) }
		var isInk: Bool { alpha > 128 && red > 128 }
	}

	private struct Bitmap {
		let width: Int
		let height: Int
		let scale: CGFloat
		let bytes: [UInt8]

		func pixel(x: Int, y: Int) -> Pixel {
			let offset = (y * width + x) * 4
			return Pixel(red: bytes[offset], green: bytes[offset + 1], blue: bytes[offset + 2], alpha: bytes[offset + 3])
		}
	}

	private static let renderScale: CGFloat = 4

	private static func render(_ view: some View, size: CGSize) -> Bitmap? {
		let renderer = ImageRenderer(content: view.frame(width: size.width, height: size.height).environment(\.colorScheme, .light))
		renderer.scale = renderScale
		guard let cgImage = renderer.cgImage else { return nil }
		let width = cgImage.width
		let height = cgImage.height
		var bytes = [UInt8](repeating: 0, count: width * height * 4)
		guard let context = CGContext(data: &bytes, width: width, height: height, bitsPerComponent: 8, bytesPerRow: width * 4, space: CGColorSpaceCreateDeviceRGB(), bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue) else { return nil }
		context.draw(cgImage, in: CGRect(x: 0, y: 0, width: width, height: height))
		return Bitmap(width: width, height: height, scale: renderScale, bytes: bytes)
	}

	/// The bounds, in points, of the light pixels a mark's number draws.
	private static func inkBounds(of mark: NumberMark, size: CGFloat) -> CGRect? {
		guard let bitmap = render(mark, size: CGSize(width: size, height: size)) else { return nil }
		var minX = Int.max, minY = Int.max, maxX = -1, maxY = -1
		for y in 0 ..< bitmap.height {
			for x in 0 ..< bitmap.width where bitmap.pixel(x: x, y: y).isInk {
				minX = min(minX, x); maxX = max(maxX, x)
				minY = min(minY, y); maxY = max(maxY, y)
			}
		}
		guard maxX >= 0 else { return nil }
		let scale = bitmap.scale
		return CGRect(x: CGFloat(minX) / scale, y: CGFloat(minY) / scale, width: CGFloat(maxX - minX + 1) / scale, height: CGFloat(maxY - minY + 1) / scale)
	}

	private static func pixels(of bitmap: Bitmap, in rect: CGRect? = nil) -> [Pixel] {
		let area = rect.map { CGRect(x: $0.minX * bitmap.scale, y: $0.minY * bitmap.scale, width: $0.width * bitmap.scale, height: $0.height * bitmap.scale) }
			?? CGRect(x: 0, y: 0, width: bitmap.width, height: bitmap.height)
		var result: [Pixel] = []
		for y in Int(area.minY) ..< Int(area.maxY) {
			for x in Int(area.minX) ..< Int(area.maxX) { result.append(bitmap.pixel(x: x, y: y)) }
		}
		return result
	}

	/// The pixels within `inset` points of the edge, all round.
	private static func ring(of bitmap: Bitmap, inset: CGFloat) -> [Pixel] {
		let band = Int(inset * bitmap.scale)
		var result: [Pixel] = []
		for y in 0 ..< bitmap.height {
			for x in 0 ..< bitmap.width where x < band || y < band || x >= bitmap.width - band || y >= bitmap.height - band {
				result.append(bitmap.pixel(x: x, y: y))
			}
		}
		return result
	}
}
