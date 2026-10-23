import SwiftUI

extension EnvironmentValues {
	/// The app's screen-share mode, set once at the root of every window; `nil` where none is set.
	@Entry public var screenShare: ScreenShareModel?
}

/// What every private-time surface says: one definition of the hand, its
/// tooltip, and what takes the value's place while screen-share mode is on.
///
/// While the mode is on a private value is not drawn at all. The hand stays
/// (or draws on, where the surface did not carry it) with the caption
/// "Private" beside it. The hand reads as "kept to yourself" and needs no
/// slash; a crossed hand would read as the opposite, that the value is
/// allowed out. The caption names the state in words for VoiceOver and for a
/// person watching the share.
public enum PrivateTimeMark {
	/// The hand, the closest system symbol until the custom one is authored.
	public static var mark: IconMark { TypeMark.privateTime.mark }

	public static let tooltip = "Private: real timer time. Hidden in screen-share mode and never in a report"
	public static let caption = "Private"

	/// The text a private value shows: the value, or the caption while hidden.
	/// A labelled value reads "Tracked 3h 10m", hidden "Tracked · private".
	public static func displayedText(value: String, label: String?, isHidden: Bool) -> String {
		if isHidden {
			return label.map { "\($0) · private" } ?? caption
		}
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
		let isHidden = screenShare?.isOn ?? false
		return HStack(spacing: isHidden || carriesHand ? 4 : 0) {
			PrivateHandGlyph(isShown: isHidden || carriesHand)
			Text(PrivateTimeMark.displayedText(value: value, label: label, isHidden: isHidden))
		}
		.help(isHidden || carriesHand ? PrivateTimeMark.tooltip : "")
		.accessibilityElement(children: .combine)
	}
}
