import Foundation

/// How the shortcut caps arrive and leave with the selection.
public enum ShortcutCapMotion: Equatable, Sendable {
	/// They grow from their leading edge over ``growDuration``.
	case grow(duration: Double)
	/// Reduce Motion: a plain crossfade, as the system does.
	case crossfade(duration: Double)
	/// The Instant setting: they appear and leave at once.
	case instant

	/// The grow transition's length, in seconds.
	public static let growDuration = 0.16
	/// The crossfade's length under Reduce Motion, the design system's 150 ms.
	public static let crossfadeDuration = 0.15

	/// The motion for the Settings choice and the system's Reduce Motion.
	public init(setting: ShortcutCapsSetting, reduceMotion: Bool) {
		switch (setting, reduceMotion) {
		case (.instant, _): self = .instant
		case (.animated, true): self = .crossfade(duration: Self.crossfadeDuration)
		case (.animated, false): self = .grow(duration: Self.growDuration)
		}
	}
}
