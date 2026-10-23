import Foundation

/// A `vaultday://` link. The scheme is registered; `vaultday://story/<number>`
/// opens the story screen (PR 15). What the other links open (a task, a day,
/// Claude Code) is decided with the surfaces that need them, so they are
/// recognised and nothing else happens yet.
public enum VaultdayLink: Equatable, Sendable {
	/// `vaultday://story/<number>`: the story screen of that story.
	case story(number: String)
	/// A `vaultday://` link this build does not act on yet: its host and path.
	case notHandledYet(host: String, path: String)

	public static let scheme = "vaultday"

	/// The link for `url`, `nil` for any other scheme.
	public init?(_ url: URL) {
		guard url.scheme?.lowercased() == Self.scheme else { return nil }
		let host = url.host() ?? ""
		let path = url.path()
		let number = path.split(separator: "/").first.map(String.init) ?? ""
		if host.lowercased() == "story", !number.isEmpty, path.split(separator: "/").count == 1 {
			self = .story(number: number)
		} else {
			self = .notHandledYet(host: host, path: path)
		}
	}

	/// A link of this app: `vaultday://<host><path>`, the path percent-encoded.
	public static func url(host: String, path: String) -> URL? {
		var components = URLComponents()
		components.scheme = scheme
		components.host = host
		components.path = path
		return components.url
	}
}
