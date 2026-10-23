import CoreGraphics
import Foundation
import XCTest
@testable import VaultdayCore

/// Vim motions in the setup flow: moving by place between a step's
/// controls, the step list, ⏎ and Esc, without views.
@MainActor
final class SetupKeyboardTests: XCTestCase {
	/// A Workspaces-like step: two cards side by side, a switch under the
	/// left one, a wide field, and the footer's Back and Continue.
	private let layout: [String: CGRect] = [
		"work": CGRect(x: 0, y: 100, width: 200, height: 120),
		"life": CGRect(x: 220, y: 100, width: 200, height: 120),
		"switch": CGRect(x: 0, y: 240, width: 60, height: 24),
		"field": CGRect(x: 0, y: 290, width: 420, height: 28),
		"back": CGRect(x: 0, y: 600, width: 80, height: 28),
		"continue": CGRect(x: 500, y: 600, width: 110, height: 28),
	]

	private func next(from name: String, _ direction: SpatialDirection) -> String? {
		let names = Array(layout.keys)
		let frames = names.map { layout[$0] ?? .zero }
		guard let current = layout[name] else { return nil }
		return SpatialNavigation.next(from: current, direction, among: frames).map { names[$0] }
	}

	func testControlsAreWalkedByPlace() {
		XCTAssertEqual(next(from: "work", .right), "life", "l along the row")
		XCTAssertEqual(next(from: "life", .left), "work")
		XCTAssertNil(next(from: "work", .left), "nothing left of the first card")
		XCTAssertEqual(next(from: "work", .down), "switch", "j to the next row, closest across")
		XCTAssertEqual(next(from: "life", .down), "switch", "the row below holds only the switch")
		XCTAssertEqual(next(from: "switch", .down), "field")
		XCTAssertEqual(next(from: "field", .down), "back", "the footer comes last; Back is closest across")
		XCTAssertEqual(next(from: "back", .right), "continue")
		XCTAssertEqual(next(from: "continue", .up), "field")
		XCTAssertNil(next(from: "continue", .down))
		let frames = Array(layout.values)
		XCTAssertEqual(SpatialNavigation.first(among: frames).map { frames[$0] }, layout["work"])
		XCTAssertNil(SpatialNavigation.first(among: []))
	}

	func testTheModelWalksTheStepThenTheStepList() {
		let model = SetupKeyboardModel()
		var ids: [String: UUID] = [:]
		for (name, frame) in layout {
			let id = UUID()
			ids[name] = id
			model.place(id, at: frame)
		}
		model.setSteps(count: 12, reachable: 3, current: 1)

		XCTAssertEqual(model.apply(.open), .continueSetup, "⏎ with nothing highlighted continues")
		XCTAssertEqual(model.apply(.moveDown), .none)
		XCTAssertEqual(model.highlighted, ids["work"], "j from the step starts at the top")
		_ = model.apply(.moveRight)
		XCTAssertEqual(model.highlighted, ids["life"])
		XCTAssertEqual(model.apply(.open), ids["life"].map(SetupKeyEffect.activate), "⏎ presses the highlighted control")
		_ = model.apply(.dismiss)
		XCTAssertNil(model.highlighted, "Esc drops the highlight, one level up")
		_ = model.apply(.moveUp)
		XCTAssertEqual(model.highlighted, ids["continue"], "k from the step starts at the end")

		_ = model.apply(.moveFirst)
		_ = model.apply(.moveLeft)
		XCTAssertEqual(model.area, .steps, "h past the left edge enters the step list")
		XCTAssertTrue(model.isHighlighted(stepRow: 1), "on the shown step")
		_ = model.apply(.moveDown)
		_ = model.apply(.moveDown)
		XCTAssertTrue(model.isHighlighted(stepRow: 2), "j stops at the last step reached")
		_ = model.apply(.moveUp)
		XCTAssertEqual(model.apply(.open), .openStep(1))
		XCTAssertEqual(model.area, .step)

		if let field = ids["field"] {
			model.place(field, at: layout["field"] ?? .zero)
			_ = model.apply(.moveLast)
			_ = model.apply(.moveUp)
			XCTAssertEqual(model.highlighted, field)
			model.remove(field)
			XCTAssertNil(model.highlighted, "a control that leaves takes its highlight")
		}
	}
}
