import Foundation

/// The stories worked on last, newest first, for the palette's empty
/// state: a story moves to the front when one of its tasks is opened,
/// peeked or timed. Kept for the app's run, per workspace.
public struct RecentStories: Equatable, Sendable {
	public static let limit = 20

	public private(set) var stories: [PaletteStory] = []

	public init() {}

	/// The story was touched now.
	public mutating func touch(_ story: PaletteStory) {
		stories.removeAll { $0.number == story.number }
		stories.insert(story, at: 0)
		if stories.count > Self.limit { stories.removeLast(stories.count - Self.limit) }
	}

	/// The stories touched so far, then `others` (the day's) not among them.
	public func filled(with others: [PaletteStory]) -> [PaletteStory] {
		var seen = Set(stories.map(\.number))
		return stories + others.filter { seen.insert($0.number).inserted }
	}

	public mutating func clear() {
		stories = []
	}
}
