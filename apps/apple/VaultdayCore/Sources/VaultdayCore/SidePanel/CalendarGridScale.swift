import Foundation

/// The hours a day grid shows and where a minute lands on it: the working
/// day rounded out to whole hours, widened to every block, at least eight
/// hours tall. Drops snap to a quarter hour.
public struct CalendarGridScale: Equatable, Sendable {
	/// The first hour row.
	public let firstHour: Int
	/// The hour the grid ends at, exclusive.
	public let endHour: Int

	public static let snapMinutes = 15
	private static let minimumHours = 8

	public init(firstHour: Int, endHour: Int) {
		self.firstHour = max(0, min(firstHour, 23))
		self.endHour = max(self.firstHour + 1, min(endHour, 24))
	}

	/// The grid for the working day `workingStart` to `workingEnd` (minutes
	/// since midnight) holding every block of `days`.
	public static func fitting(days: [CalendarDay], workingStart: Int, workingEnd: Int) -> CalendarGridScale {
		let blocks = days.flatMap(\.blocks)
		let earliest = ([workingStart] + blocks.map(\.startMinute)).min() ?? workingStart
		let latest = ([workingEnd] + blocks.map(\.endMinute)).max() ?? workingEnd
		var first = earliest / 60
		var end = (latest + 59) / 60
		while end - first < minimumHours {
			if end < 24 { end += 1 }
			if end - first < minimumHours, first > 0 { first -= 1 }
			if first == 0, end == 24 { break }
		}
		return CalendarGridScale(firstHour: first, endHour: end)
	}

	public var hours: [Int] { Array(firstHour ..< endHour) }
	public var totalMinutes: Int { (endHour - firstHour) * 60 }

	/// "08:00", the label of an hour row.
	public static func label(hour: Int) -> String {
		String(format: "%02d:00", hour)
	}

	/// The offset of `minute` on a grid `height` points tall, clamped to it.
	public func y(forMinute minute: Int, height: Double) -> Double {
		let offset = Double(min(max(minute - firstHour * 60, 0), totalMinutes))
		return offset / Double(totalMinutes) * height
	}

	/// The minute at `y` on a grid `height` points tall, snapped to a quarter
	/// hour and kept inside the grid.
	public func minute(atY y: Double, height: Double) -> Int {
		guard height > 0 else { return firstHour * 60 }
		let raw = Double(firstHour * 60) + y / height * Double(totalMinutes)
		let snapped = Int((raw / Double(Self.snapMinutes)).rounded()) * Self.snapMinutes
		return min(max(snapped, firstHour * 60), endHour * 60 - Self.snapMinutes)
	}
}
