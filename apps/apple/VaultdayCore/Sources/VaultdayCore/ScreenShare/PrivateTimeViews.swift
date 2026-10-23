import SwiftUI

extension EnvironmentValues {
	/// The app's screen-share mode, set once at the root of every window; `nil` where none is set.
	@Entry public var screenShare: ScreenShareModel?
}

/// What every private-time surface says: one definition of the hand and its
/// tooltip, and what a private value shows while screen-share mode is on.
///
/// While the mode is on a private value is not drawn at all, and nothing
/// takes its place: no hand, no "Private" caption. A caption saying that
/// something is withheld would tell the people watching the share exactly
/// that, which defeats the mode; the surface reads as if it had no such value.
/// The hand reads as "kept to yourself" beside a shown value and needs no
/// slash; a crossed hand would read as the opposite.
public enum PrivateTimeMark {
	/// The hand, the closest system symbol until the custom one is authored.
	public static var mark: IconMark { TypeMark.privateTime.mark }

	public static let tooltip = "Private: real timer time. Hidden in screen-share mode and never in a report"

	/// The text a private value shows: the value, labelled ("Tracked 3h 10m"),
	/// or nothing at all while screen-share mode hides it.
	public static func displayedText(value: String, label: String?, isHidden: Bool) -> String? {
		guard !isHidden else { return nil }
		return label.map { "\($0) \(value)" } ?? value
	}
}

/// The hand glyph. It draws on when it appears and off when it leaves, after
/// the view's first render only, as every icon in the app moves; an absent
/// glyph takes no room.
public struct PrivateHandGlyph: View {
	let isShown: Bool
	let size: CGFloat

	public init(isShown: Bool = true, size: CGFloat = 10) {
		self.isShown = isShown
		self.size = size
	}

	public var body: some View {
		IconPresence(isShown: isShown) {
			Image(systemName: PrivateTimeMark.mark.symbol)
				.font(.system(size: size))
				.accessibilityHidden(true)
		}
		.frame(width: isShown ? size + 2 : 0, height: size + 2)
	}
}

/// A private time value with the mode applied. `carriesHand` is for the
/// surfaces that always show the hand beside tracked time (the Hours tabs and
/// the story screen); the others show the hand only while the mode hides the
/// value.
public struct PrivateTimeValue: View {
	let value: String
	let label: String?
	let carriesHand: Bool
	@Environment(\.screenShare) private var screenShare

	public init(_ value: String, label: String? = nil, carriesHand: Bool = false) {
		self.value = value
		self.label = label
		self.carriesHand = carriesHand
	}

	public var body: some View {
		if let text = PrivateTimeMark.displayedText(value: value, label: label, isHidden: screenShare?.isOn ?? false) {
			HStack(spacing: carriesHand ? 4 : 0) {
				PrivateHandGlyph(isShown: carriesHand)
				Text(text)
			}
			.help(carriesHand ? PrivateTimeMark.tooltip : "")
			.accessibilityElement(children: .combine)
		}
	}
}
