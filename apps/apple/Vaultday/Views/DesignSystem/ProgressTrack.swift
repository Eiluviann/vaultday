import SwiftUI

/// A thin linear meter: the share of a target reached, on a pill track one
/// step darker than the card it sits on. Nothing is filled at zero; the
/// system bar draws its rounded cap there, which reads as progress made.
struct ProgressTrack: View {
	/// The share reached, 0...1; values outside are clamped.
	let value: Double
	var tint: Color = Tokens.Colors.primary
	var track: Color = Tokens.Colors.fillControlSoft

	var body: some View {
		GeometryReader { proxy in
			let width = ProgressTrack.fillWidth(value: value, trackWidth: proxy.size.width)
			ZStack(alignment: .leading) {
				Capsule().fill(track)
				if width > 0 {
					Capsule().fill(tint).frame(width: width)
				}
			}
		}
		.frame(height: ProgressTrack.height)
		.accessibilityValue(Text("\(Int((min(max(value, 0), 1) * 100).rounded())) percent"))
	}

	static let height: CGFloat = 6

	/// The filled width: none at zero (or below), the full track at one (or
	/// above), and never narrower than the track is tall when anything is
	/// reached, so a small share still reads as a pill and not a dot.
	static func fillWidth(value: Double, trackWidth: CGFloat) -> CGFloat {
		guard value > 0, trackWidth > 0, value.isFinite else { return 0 }
		let share = min(value, 1)
		return min(trackWidth, max(height, trackWidth * share))
	}
}
