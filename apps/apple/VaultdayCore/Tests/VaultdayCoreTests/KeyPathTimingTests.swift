import Foundation
import XCTest
@testable import VaultdayCore

/// The model's share of a key's latency budget (1 ms of the 16): the
/// dispatcher and the focus tree, timed without views over many strokes.
@MainActor
final class KeyPathTimingTests: XCTestCase {
	private static let strokes = 2000

	func testTheKeyPathStaysUnderAMillisecond() throws {
		let text = try String(contentsOf: TestFixtures.repoRoot.appending(path: "apps/apple/Keymap.yaml"), encoding: .utf8)
		let keymap = try Keymap.parse(text).get()
		let focus = PaneFocusModel(scheme: .vim)
		let session = KeyboardSession(dispatcher: KeyDispatcher(keymap: keymap, scheme: .vim), focus: focus, register: PlanRegister(plan: FakePlan()) { _ in })
		focus.setItemCount(40, in: .today)
		focus.setItemCount(9, in: .sidebar)
		focus.setControlCount(6, in: .dayHeader)

		let sequences: [(name: String, strokes: [String])] = [
			("j k", ["j", "k"]),
			("g1 g2", ["g", "1", "g", "2"]),
			("⌘K", ["cmd+k"]),
		]
		var report: [String] = []
		for (name, notations) in sequences {
			let strokes = notations.compactMap { KeyStroke(notation: $0) }
			let clock = ContinuousClock()
			let elapsed = clock.measure {
				for index in 0 ..< Self.strokes {
					session.handle(strokes[index % strokes.count])
				}
			}
			let perStroke = Double(elapsed.components.attoseconds) / 1e15 / Double(Self.strokes) + Double(elapsed.components.seconds) * 1000 / Double(Self.strokes)
			report.append("\(name): \(String(format: "%.4f", perStroke)) ms per stroke")
			XCTAssertLessThan(perStroke, 1, "\(name) takes \(perStroke) ms in the model alone")
		}
		print("Key path, dispatcher and focus tree only:\n" + report.joined(separator: "\n"))
	}
}
