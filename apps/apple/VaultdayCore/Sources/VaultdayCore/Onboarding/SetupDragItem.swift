import Foundation

/// Where a folder moving to another workspace comes from.
public enum FolderOrigin: Hashable, Sendable {
	/// A folder the workspace holds a grant for: linked into its root, or with its link missing.
	case linked(folderId: String)
	/// A plain folder directly in the workspace's root.
	case inside(name: String)
}

/// What the setup steps drag: a workspace card to the other sphere, or a
/// folder row to another workspace. Carried as plain text, so a drop of
/// anything else is ignored.
public enum SetupDragItem: Hashable, Sendable {
	case workspace(id: String)
	case folder(workspaceId: String, origin: FolderOrigin)

	private static let prefix = "vaultday-setup"
	private static let separator: Character = "\u{1F}"

	/// The text the drag carries.
	public var text: String {
		let parts = switch self {
		case let .workspace(id): ["workspace", id]
		case let .folder(workspaceId, .linked(folderId)): ["folder", workspaceId, "linked", folderId]
		case let .folder(workspaceId, .inside(name)): ["folder", workspaceId, "inside", name]
		}
		return ([Self.prefix] + parts).joined(separator: String(Self.separator))
	}

	/// The item a drag's text names; `nil` for text that is not one.
	public init?(text: String) {
		let parts = text.split(separator: Self.separator, omittingEmptySubsequences: false).map(String.init)
		guard parts.first == Self.prefix, parts.dropFirst().allSatisfy({ !$0.isEmpty }) else { return nil }
		let fields = Array(parts.dropFirst())
		if fields.count == 2, fields[0] == "workspace" {
			self = .workspace(id: fields[1])
		} else if fields.count == 4, fields[0] == "folder", fields[2] == "linked" {
			self = .folder(workspaceId: fields[1], origin: .linked(folderId: fields[3]))
		} else if fields.count == 4, fields[0] == "folder", fields[2] == "inside" {
			self = .folder(workspaceId: fields[1], origin: .inside(name: fields[3]))
		} else {
			return nil
		}
	}
}
