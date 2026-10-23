import AppKit
import Foundation
import XCTest
@testable import VaultdayCore

/// A window with a sharing type and nothing else.
@MainActor
private final class FakeWindow: SharableWindow {
	var sharingType: NSWindow.SharingType = .readOnly
}

/// Screen-share mode: what hides, the workspace rules, restoration, the
/// window sharing type, the report variant, the keymap binding and the
/// per-device persistence.
@MainActor
final class ScreenShareTests: XCTestCase {
	private let work = WorkspaceEntry(id: "ws-work", name: "Work", sphere: .work, accent: nil, rootPath: "/a", folders: [])
	private let acme = WorkspaceEntry(id: "ws-acme", name: "Acme", sphere: .work, accent: nil, rootPath: "/b", folders: [])
	private let home = WorkspaceEntry(id: "ws-home", name: "Personal", sphere: .life, accent: nil, rootPath: "/c", folders: [])

	private var entries: [WorkspaceEntry] { [work, acme, home] }

	private func model(isOn: Bool = false) -> (ScreenShareModel, Persisted) {
		let model = ScreenShareModel()
		let persisted = Persisted()
		model.persist = { persisted.writes.append(Write(isOn: $0, returnTo: $1)) }
		model.restore(isOn: isOn, returnTo: nil)
		return (model, persisted)
	}

	private struct Write: Equatable {
		let isOn: Bool
		let returnTo: WorkspaceSelection?
	}

	@MainActor
	private final class Persisted {
		var writes: [Write] = []
	}

	// MARK: What hides

	func testEverySurfaceHidesWhileOnAndNoneWhileOff() {
		let (model, _) = model()
		for surface in ScreenShareSurface.allCases {
			XCTAssertFalse(model.hides(surface), "\(surface) off")
		}
		_ = model.setOn(true, current: .workspace("ws-work"), entries: entries)
		for surface in ScreenShareSurface.allCases {
			XCTAssertTrue(model.hides(surface), "\(surface) on")
		}
		XCTAssertEqual(ScreenShareSurface.allCases.filter(\.isTrackedTime).count, 9)
		// The mini tracker and the menu-bar window are kept out of a capture
		// instead, and keep their values: no surface stands for them.
		XCTAssertFalse(ScreenShareSurface.allCases.map { "\($0)" }.contains { $0.contains("miniTracker") || $0 == "menuBar" })
	}

	func testAPrivateTimeValueIsReplacedByThePrivateCaptionWhileHidden() {
		XCTAssertEqual(PrivateTimeMark.displayedText(value: "3h 10m", label: nil, isHidden: false), "3h 10m")
		XCTAssertEqual(PrivateTimeMark.displayedText(value: "3h 10m", label: "Tracked", isHidden: false), "Tracked 3h 10m")
		XCTAssertEqual(PrivateTimeMark.displayedText(value: "3h 10m", label: nil, isHidden: true), "Private")
		XCTAssertEqual(PrivateTimeMark.displayedText(value: "3h 10m", label: "Tracked", isHidden: true), "Tracked · private")
		for hidden in [true, false] {
			XCTAssertFalse(PrivateTimeMark.displayedText(value: "3h 10m", label: "Tracked", isHidden: hidden).isEmpty)
		}
		XCTAssertTrue(PrivateTimeMark.displayedText(value: "3h 10m", label: "Tracked", isHidden: true).contains("3h") == false)
		XCTAssertEqual(PrivateTimeMark.mark.symbol, "hand.raised")
	}

	func testTheLongTimerNotificationNamesNeitherTaskNorTimeWhileSharing() {
		XCTAssertEqual(UserNotificationLongTimerAlert.body(subject: "Write the script", elapsedSeconds: 3 * 3600, isScreenShareOn: false), "Write the script has run for 3h")
		let shared = UserNotificationLongTimerAlert.body(subject: "Write the script", elapsedSeconds: 3 * 3600, isScreenShareOn: true)
		XCTAssertFalse(shared.contains("script"))
		XCTAssertFalse(shared.contains("3h"))
	}

	func testTheDayLogLeavesOutSessionLinesWhileOn() {
		let log = DayLogModel()
		XCTAssertFalse(log.hidesSessionLines)
		log.hidesSessionLines = true
		XCTAssertTrue(log.visibleLines.allSatisfy { $0.kind != "session" })
	}

	func testASignalCardKeepsOnlyItsHeadlineAndSource() {
		let card = SignalCard(
			id: "sig-1", source: .outlook, sender: "Maya Chen", senderAddress: "maya@example.test", receivedAt: nil, timeLabel: "09:42",
			headline: "Checkout copy", subject: "RE: Checkout copy WEB-142", excerpt: "Can you send the figures", ticket: "WEB-142",
			ticketStoryTitle: "Checkout redesign", state: .new,
		)
		let shared = card.reducedForScreenShare
		XCTAssertEqual(shared.headline, "Checkout copy")
		XCTAssertEqual(shared.source, .outlook)
		XCTAssertEqual(shared.id, "sig-1")
		XCTAssertEqual(shared.state, .new)
		for hidden in [shared.sender, shared.senderAddress, shared.timeLabel, shared.excerpt] {
			XCTAssertEqual(hidden, "")
		}
		XCTAssertEqual(shared.subject, "Checkout copy")
		XCTAssertNil(shared.ticket)
		XCTAssertNil(shared.ticketStoryTitle)
		XCTAssertEqual(shared.waitForLabel, "Wait for")
		XCTAssertEqual(card.waitForLabel, "Wait for Maya")
	}

	func testPeekAndPreviewKeepWhatTheRowShowsAndNothingMore() {
		let peek = PeekContent(
			chips: ["task-2"], subtitle: "Checkout", badge: "waiting on Janusz · 6d", title: "Wire the form",
			fields: [.init(label: "Estimate", value: "1h"), .init(label: "Assigned to", value: "Janusz"), .init(label: "Created", value: "3 Oct")],
			excerpt: "A paragraph of the note", activity: [PreviewActivity(id: "a", whenLabel: "09:30", kind: "note", text: "Called Janusz", agent: nil)],
			taskId: "task-2", storyNumber: "WEB-142", obsidianLink: nil,
		)
		let reducedPeek = peek.reducedForScreenShare
		XCTAssertEqual(reducedPeek.fields.map(\.label), ["Estimate"])
		XCTAssertNil(reducedPeek.excerpt)
		XCTAssertNil(reducedPeek.badge)
		XCTAssertTrue(reducedPeek.activity.isEmpty)
		XCTAssertEqual(reducedPeek.title, "Wire the form")
		XCTAssertEqual(reducedPeek.chips, ["task-2"])

		let preview = TaskPreviewContent(
			taskId: "task-2", text: "Wire the form", state: .open, storyNumber: "WEB-142", storyTitle: "Checkout", storyStatus: nil,
			sectionTitle: nil, estimateLabel: "1h", scheduledLabel: nil, dueLabel: nil, assignedTo: "Janusz",
			blocks: [.paragraph("note text")], subtasks: [], waiting: [PreviewWaiting(taskId: "t", text: "x", assignedTo: "Maya", ageDays: 2)],
			activity: [PreviewActivity(id: "a", whenLabel: "09:30", kind: nil, text: "x", agent: nil)], obsidianLink: nil, obsidianLinkNotice: nil,
		)
		let reducedPreview = preview.reducedForScreenShare
		XCTAssertTrue(reducedPreview.blocks.isEmpty)
		XCTAssertTrue(reducedPreview.waiting.isEmpty)
		XCTAssertTrue(reducedPreview.activity.isEmpty)
		XCTAssertNil(reducedPreview.assignedTo)
		XCTAssertEqual(reducedPreview.estimateLabel, "1h")
		XCTAssertEqual(reducedPreview.text, "Wire the form")
	}

	// MARK: Workspaces

	func testALifeWorkspaceLeavesTheSwitcherAndRefusesSelectionWithANotice() {
		let (model, _) = model()
		XCTAssertEqual(model.visible(entries).map(\.id), ["ws-work", "ws-acme", "ws-home"])
		XCTAssertFalse(model.refusesShowing(home))
		_ = model.setOn(true, current: .workspace("ws-work"), entries: entries)
		XCTAssertEqual(model.visible(entries).map(\.id), ["ws-work", "ws-acme"])
		XCTAssertTrue(model.canShow(acme))
		XCTAssertFalse(model.refusesShowing(acme))
		XCTAssertNil(model.notice)
		XCTAssertTrue(model.refusesShowing(home))
		XCTAssertEqual(model.notice, ScreenShareModel.hiddenWorkspaceNotice)
		XCTAssertFalse(model.notice?.contains("Personal") ?? true, "the notice never names the hidden workspace")
		model.dismissNotice()
		XCTAssertNil(model.notice)
	}

	func testAllWorkspacesIsOffWithTheDesignsReasonWhileOn() {
		let (model, _) = model()
		XCTAssertNil(model.allWorkspacesDisabledReason)
		XCTAssertFalse(model.refusesShowingAll())
		_ = model.setOn(true, current: .workspace("ws-work"), entries: entries)
		XCTAssertEqual(model.allWorkspacesDisabledReason, "Screen-share mode needs a single workspace: pick one first")
		XCTAssertTrue(model.refusesShowingAll())
		XCTAssertEqual(model.notice, model.allWorkspacesDisabledReason)
	}

	func testTheModeStartsOnlyFromAWorkWorkspaceAndIsRefusedOnAllOrALifeOneWithTheReason() {
		let (model, persisted) = model()
		XCTAssertNil(model.startDisabledReason(current: .workspace("ws-work"), entries: entries))
		for hidden in [WorkspaceSelection.all, .workspace("ws-home")] {
			XCTAssertEqual(model.startDisabledReason(current: hidden, entries: entries), ScreenShareModel.startReason, "the header draws the toggle disabled on \(hidden)")
			XCTAssertEqual(model.setOn(true, current: hidden, entries: entries), .refused(ScreenShareModel.startReason))
			XCTAssertFalse(model.isOn)
			XCTAssertEqual(model.notice, ScreenShareModel.startReason)
		}
		XCTAssertEqual(model.startDisabledReason(current: nil, entries: entries), ScreenShareModel.startReason, "no workspace open")
		XCTAssertTrue(persisted.writes.isEmpty)
		XCTAssertNil(model.returnTo, "a refused start remembers no way back")
	}

	func testOnceOnTheToggleIsNeverDisabledAndASwitchToALifeWorkspaceOrAllIsRefused() {
		let (model, _) = model()
		_ = model.setOn(true, current: .workspace("ws-work"), entries: entries)
		XCTAssertNil(model.startDisabledReason(current: .workspace("ws-work"), entries: entries))
		XCTAssertTrue(model.refusesShowing(home))
		XCTAssertTrue(model.refusesShowingAll())
		XCTAssertTrue(model.isOn)
	}

	func testTurningOnInAWorkWorkspaceMovesNothingAndOffRestoresNothing() {
		let (model, persisted) = model()
		XCTAssertEqual(model.setOn(true, current: .workspace("ws-acme"), entries: entries), .none)
		XCTAssertNil(model.returnTo)
		XCTAssertEqual(persisted.writes, [Write(isOn: true, returnTo: nil)])
		XCTAssertEqual(model.setOn(false, current: .workspace("ws-work"), entries: entries), .none)
	}

	func testTurningOffAfterARelaunchMovedAwayGoesBack() {
		let (model, persisted) = model()
		model.restore(isOn: true, returnTo: "workspace:ws-home")
		XCTAssertEqual(model.setOn(false, current: .workspace("ws-work"), entries: entries), .select(.workspace("ws-home")))
		XCTAssertNil(model.returnTo)
		XCTAssertEqual(persisted.writes.last, Write(isOn: false, returnTo: nil))
	}

	func testTheWayBackIsDroppedWhenItsWorkspaceIsGone() {
		let (model, _) = model()
		model.restore(isOn: true, returnTo: "workspace:ws-home")
		XCTAssertEqual(model.setOn(false, current: .workspace("ws-work"), entries: [work, acme]), .none)
	}

	func testTheModeIsRefusedWithoutAWorkWorkspace() {
		let (model, persisted) = model()
		let change = model.setOn(true, current: .workspace("ws-home"), entries: [home])
		XCTAssertEqual(change, .refused(ScreenShareModel.startReason))
		XCTAssertFalse(model.isOn)
		XCTAssertTrue(persisted.writes.isEmpty)
	}

	func testSettingsListsLeaveTheLifeWorkspacesOutWhileOn() {
		let (model, _) = model()
		XCTAssertEqual(WorkspaceSphere.allCases.filter(model.showsSphere), [.work, .life])
		_ = model.setOn(true, current: .workspace("ws-work"), entries: entries)
		XCTAssertEqual(WorkspaceSphere.allCases.filter(model.showsSphere), [.work])
	}

	func testTheCloseOutRefusesToOpenWithANoticeWhileOn() {
		let (model, _) = model()
		XCTAssertFalse(model.refusesClosingOut())
		XCTAssertNil(model.notice)
		_ = model.setOn(true, current: .workspace("ws-work"), entries: entries)
		XCTAssertTrue(model.refusesClosingOut())
		XCTAssertEqual(model.notice, ScreenShareModel.closeOutNotice)
		XCTAssertFalse(ScreenShareModel.closeOutNotice.hasSuffix("."))
	}

	func testSettingTheSameStateAgainChangesNothing() {
		let (model, persisted) = model(isOn: true)
		XCTAssertEqual(model.setOn(true, current: .workspace("ws-work"), entries: entries), .none)
		XCTAssertTrue(persisted.writes.isEmpty)
	}

	func testARelaunchWithTheModeOnMovesAHiddenWorkspaceAndKeepsTheWayBack() {
		let (model, persisted) = model()
		model.restore(isOn: true, returnTo: "workspace:ws-home")
		XCTAssertEqual(model.returnTo, .workspace("ws-home"))
		XCTAssertEqual(model.enforce(current: .workspace("ws-home"), entries: entries), .select(.workspace("ws-work")))
		XCTAssertEqual(model.returnTo, .workspace("ws-home"), "the earlier way back stands")
		XCTAssertEqual(persisted.writes.last?.returnTo, .workspace("ws-home"))
		XCTAssertEqual(model.enforce(current: .workspace("ws-acme"), entries: entries), .none)
	}

	func testARelaunchWithTheModeOnAndNoWorkWorkspaceLeftTurnsTheModeOff() {
		let (model, persisted) = model()
		model.restore(isOn: true, returnTo: "workspace:ws-home")
		XCTAssertEqual(model.enforce(current: .workspace("ws-home"), entries: [home]), .none)
		XCTAssertFalse(model.isOn, "the mode does not sit on a workspace it hides")
		XCTAssertNil(model.returnTo)
		XCTAssertEqual(model.notice, ScreenShareModel.noWorkWorkspaceNotice)
		XCTAssertEqual(persisted.writes, [Write(isOn: false, returnTo: nil)])
		XCTAssertEqual(model.enforce(current: .workspace("ws-home"), entries: [home]), .none, "off, nothing more to do")
		XCTAssertEqual(persisted.writes.count, 1)

		let (opened, untouched) = self.model(isOn: true)
		XCTAssertEqual(opened.enforce(current: nil, entries: [home]), .none, "no workspace open yet")
		XCTAssertTrue(opened.isOn)
		XCTAssertTrue(untouched.writes.isEmpty)
	}

	func testTheToggleTooltipNamesItsKeyWithoutCuttingAFormattedString() {
		XCTAssertEqual(
			ScreenShareModel.toggleTooltip(isOn: true, keyLabel: "⇧⌘S"),
			"Screen-share mode is on: private workspaces are hidden and tracked time shows as private; ⇧⌘S turns it off",
		)
		XCTAssertEqual(
			ScreenShareModel.toggleTooltip(isOn: true, keyLabel: nil),
			"Screen-share mode is on: private workspaces are hidden and tracked time shows as private",
		)
		XCTAssertEqual(
			ScreenShareModel.toggleTooltip(isOn: false, keyLabel: "⇧⌘S"),
			"Screen-share mode: hide other workspaces and silence their notifications (⇧⌘S)",
		)
		XCTAssertEqual(
			ScreenShareModel.toggleTooltip(isOn: false, keyLabel: nil),
			"Screen-share mode: hide other workspaces and silence their notifications",
		)
	}

	func testTheSelectionToReturnToIsKeptAsPlainText() {
		XCTAssertEqual(WorkspaceSelection.all.storedText, "all")
		XCTAssertEqual(WorkspaceSelection.workspace("ws-1").storedText, "workspace:ws-1")
		XCTAssertEqual(WorkspaceSelection(storedText: "workspace:ws-1"), .workspace("ws-1"))
		XCTAssertEqual(WorkspaceSelection(storedText: "all"), .all)
		XCTAssertNil(WorkspaceSelection(storedText: "workspace:"))
		XCTAssertNil(WorkspaceSelection(storedText: "nonsense"))
	}

	// MARK: Window sharing

	func testTheRegisteredWindowsAreExcludedWhileOnAndReadOnlyOtherwise() {
		var isOn = false
		let registry = WindowSharingRegistry()
		registry.isScreenShareOn = { isOn }
		let tracker = FakeWindow()
		let menuBar = FakeWindow()
		let main = FakeWindow()
		registry.register(tracker)
		registry.register(menuBar)
		registry.register(tracker)
		XCTAssertEqual(registry.windowCount, 2, "a window registers once")
		XCTAssertEqual(tracker.sharingType, .readOnly)

		isOn = true
		registry.apply()
		XCTAssertEqual(tracker.sharingType, .none)
		XCTAssertEqual(menuBar.sharingType, .none)
		XCTAssertEqual(main.sharingType, .readOnly, "the main window is never registered and stays shareable")

		let late = FakeWindow()
		registry.register(late)
		XCTAssertEqual(late.sharingType, .none, "a window made while the mode is on starts excluded")

		isOn = false
		registry.apply()
		for window in [tracker, menuBar, late] {
			XCTAssertEqual(window.sharingType, .readOnly)
		}
		XCTAssertEqual(WindowSharingRegistry.sharingType(isScreenShareOn: true), .none)
		XCTAssertEqual(WindowSharingRegistry.sharingType(isScreenShareOn: false), .readOnly)
	}

	func testAClosedWindowLeavesTheRegistry() {
		let registry = WindowSharingRegistry()
		var window: FakeWindow? = FakeWindow()
		if let window { registry.register(window) }
		XCTAssertEqual(registry.windowCount, 1)
		window = nil
		XCTAssertEqual(registry.windowCount, 0)
	}

	// MARK: Reports

	func testTheReportsTabOffersOnlyTheFteSafeVariantWhileOn() {
		let reports = ReportsModel()
		XCTAssertEqual(reports.offeredVariants, [.full])
		XCTAssertNil(reports.variantNote)
		reports.isScreenShare = true
		XCTAssertEqual(reports.offeredVariants, [.fteSafe])
		XCTAssertEqual(reports.variant, .fteSafe)
		XCTAssertNotNil(reports.variantNote)
		reports.isScreenShare = false
		XCTAssertEqual(reports.variant, .full)
	}

	func testTheFteSafeVariantBuildsLoggedHoursOnlyFromAnHourlyWorkspace() throws {
		let root = try TestFixtures.copyOfMockVault(self)
		let settings = root.appending(path: ".vaultday/settings.json")
		var object = try XCTUnwrap(JSONSerialization.jsonObject(with: Data(contentsOf: settings)) as? [String: Any])
		object["billing"] = ["model": "hourly", "rounding_minutes": 30]
		try JSONSerialization.data(withJSONObject: object).write(to: settings)
		let reports = ReportsModel(now: { DayText.date(from: "2026-10-22") ?? Date() })
		reports.attach(source: (try? openWorkspace(path: root.path)).map(CoreReportSource.init))
		reports.show(subject: "WEB-142", load: true)
		XCTAssertEqual(reports.bundle?.billing, .hourly)

		reports.isScreenShare = true
		XCTAssertEqual(reports.bundle?.billing, .fte, "turning the mode on reads the report again as FTE-safe")
		guard case .fte = reports.bundle?.hours else { return XCTFail("expected logged hours only") }
		XCTAssertFalse(reports.jsonPayload?.contains("roundingMinutes") ?? true)

		reports.isScreenShare = false
		XCTAssertEqual(reports.bundle?.billing, .hourly)
	}

	// MARK: Keymap and persistence

	func testCommandShiftSIsBoundGloballyInBothSchemesAndNothingElseUsesIt() throws {
		let text = try String(contentsOf: TestFixtures.repoRoot.appending(path: "apps/apple/Keymap.yaml"), encoding: .utf8)
		let keymap = try Keymap.parse(text).get()
		let stroke = try XCTUnwrap(KeyStroke(notation: "shift+cmd+s"))
		for scheme in KeyScheme.allCases {
			XCTAssertEqual(keymap.singleStroke(for: .toggleScreenShare, scheme: scheme), stroke)
			let users = keymap.sequences(for: scheme).filter { $0.sequence == [stroke] }
			XCTAssertEqual(users.map(\.action), [.toggleScreenShare], "no other action takes ⌘⇧S under \(scheme)")
			XCTAssertNil(users.first?.pane, "bound to no pane")
		}
	}

	func testTheModeAndItsWayBackSurviveARelaunchOnThisDeviceAlone() throws {
		let harness = try OnboardingHarness(self)
		let settings = SettingsModel(session: harness.session)
		XCTAssertFalse(settings.isScreenShareOn)
		XCTAssertNil(settings.screenShareReturnTo)

		settings.setScreenShare(isOn: true, returnTo: "workspace:ws-home")
		XCTAssertNil(settings.errorMessage)
		let reopened = SettingsModel(session: try harness.relaunched().session)
		XCTAssertTrue(reopened.isScreenShareOn)
		XCTAssertEqual(reopened.screenShareReturnTo, "workspace:ws-home")

		let model = ScreenShareModel()
		model.restore(isOn: reopened.isScreenShareOn, returnTo: reopened.screenShareReturnTo)
		XCTAssertTrue(model.isOn)
		XCTAssertEqual(model.returnTo, .workspace("ws-home"))

		reopened.setScreenShare(isOn: false, returnTo: nil)
		let off = SettingsModel(session: try harness.relaunched().session)
		XCTAssertFalse(off.isScreenShareOn)
		XCTAssertNil(off.screenShareReturnTo)
	}
}
