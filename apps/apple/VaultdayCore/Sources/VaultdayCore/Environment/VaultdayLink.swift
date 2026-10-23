import Foundation

/// A `vaultday://` link. The scheme is registered now; what links open
/// (a story, a task, a day) is decided with the surfaces that need them, so
/// every link is recognised and nothing else happens yet.
public enum VaultdayLink: Equatable, Sendable {
	/// A `vaultday://` link this build does not act on yet: its host and path.
	case notHandledYet(host: String, path: String)

	public static let scheme = "vaultday"

	/// The link for `url`, `nil` for any other scheme.
	public init?(_ url: URL) {
		guard url.scheme?.lowercased() == Self.scheme else { return nil }
		self = .notHandledYet(host: url.host() ?? "", path: url.path())
	}
}
