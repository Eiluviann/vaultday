import XCTest
@testable import VaultdayCore

/// The icon motion rules of the design note, on the model side: never on
/// first render, one effect per change, Reduce Motion crossfades; and one
/// icon shape per source and per type.
final class IconMotionTests: XCTestCase {
	func testTheFirstRenderNeverAnimates() {
		var gate = IconMotionGate<Bool>()
		XCTAssertEqual(gate.plan(.drawOn, to: true, reduceMotion: false), .none, "a view's first value is simply there")
		XCTAssertTrue(gate.hasRendered)
		XCTAssertEqual(gate.playedCount, 0)

		var rendered = IconMotionGate<String>()
		rendered.firstRender("play.fill")
		XCTAssertEqual(rendered.plan(.replace, to: "play.fill", reduceMotion: false), .none)
		XCTAssertEqual(IconMotionGate<Int>.plan(.drawOn, hasRendered: false, isChange: true, reduceMotion: false), .none)
	}

	func testEachChangePlaysItsOneEffectOnce() {
		var gate = IconMotionGate<String>()
		gate.firstRender("play.fill")
		XCTAssertEqual(gate.plan(.replace, to: "pause.fill", reduceMotion: false), .effect(.replace))
		XCTAssertEqual(gate.plan(.replace, to: "pause.fill", reduceMotion: false), .none, "the same change again plays nothing")
		XCTAssertEqual(gate.plan(.replace, to: "play.fill", reduceMotion: false), .effect(.replace))
		XCTAssertEqual(gate.playedCount, 2)
		XCTAssertEqual(IconMotionGate<Int>.plan(.drawOff, hasRendered: true, isChange: false, reduceMotion: false), .none)
	}

	func testReduceMotionCrossfadesInsteadOfDrawing() {
		var gate = IconMotionGate<Bool>()
		gate.firstRender(false)
		XCTAssertEqual(gate.plan(.drawOn, to: true, reduceMotion: true), .crossfade)
		XCTAssertEqual(gate.plan(.drawOff, to: false, reduceMotion: true), .crossfade)
		for kind in [IconMotionKind.drawOn, .drawOff, .replace, .variableDraw] {
			XCTAssertEqual(IconMotionGate<Int>.plan(kind, hasRendered: true, isChange: true, reduceMotion: true), .crossfade)
			XCTAssertEqual(IconMotionGate<Int>.plan(kind, hasRendered: true, isChange: true, reduceMotion: false), .effect(kind))
		}
		XCTAssertEqual(IconMotionTiming.crossfade, 0.15, "the design system's 150 ms transition")
	}

	func testOnlyRowsThatArriveWhileShownAreArrivals() {
		var ledger = ArrivalLedger()
		XCTAssertFalse(ledger.isArrival("sig-1"), "nothing arrives before the list shows")
		ledger.settle(["sig-1", "sig-2"])
		XCTAssertFalse(ledger.isArrival("sig-1"))
		XCTAssertTrue(ledger.isArrival("sig-3"))
		ledger.record(["sig-1", "sig-2", "sig-3"])
		XCTAssertFalse(ledger.isArrival("sig-3"), "drawn once, then known")
		ledger.settle(["other"])
		XCTAssertTrue(ledger.isArrival("other"), "settling happens once")
	}

	func testEverySourceAndTypeHasOneShapeAndTheCustomSymbolsAreMarked() {
		let sources = SourceMark.allCases.map(\.mark.symbol)
		XCTAssertEqual(Set(sources).count, sources.count, "one icon shape per source")
		let types = TypeMark.allCases.map(\.mark.symbol)
		XCTAssertEqual(Set(types).count, types.count, "one icon shape per type")
		let pending = (SourceMark.allCases.map(\.mark) + TypeMark.allCases.map(\.mark)).filter(\.isCustomSymbolPending).map(\.name)
		XCTAssertEqual(Set(pending), ["Teams", "Captured", "Decision by a model", "Chaser", "Private, never in a report"])
		XCTAssertEqual(TypeMark(dayLogKind: "decision", isByModel: true), .modelDecision)
		XCTAssertEqual(TypeMark(dayLogKind: "decision", isByModel: false), .decision)
		XCTAssertEqual(TypeMark(dayLogKind: "whatever", isByModel: false), .note)
		XCTAssertEqual(TypeMark(paletteChoice: .story("WEB-142")), .story)
		let card = SignalCard(id: "sig-1", sender: "Synthetic", senderAddress: "a@example.test", receivedAt: nil, timeLabel: "09:00", headline: "h", subject: "s", excerpt: "e", ticket: nil, ticketStoryTitle: nil, state: .new)
		XCTAssertEqual(card.source, .mail, "mail is the base version's one source")
		XCTAssertEqual(card.sourceSymbol, SourceMark.mail.mark.symbol)
	}
}
