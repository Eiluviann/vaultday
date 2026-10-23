import Foundation
import XCTest
@testable import VaultdayCore

/// Rituals and routines: DayTime, validation, and what each choice stores in the app-level document.
@MainActor
final class RhythmModelTests: XCTestCase {
	func testDayTimeReadsTheClockAndRefusesWhatIsNotOnIt() {
		XCTAssertEqual(DayTime(text: "8:45")?.minuteOfDay, 525)
		XCTAssertEqual(DayTime(text: " 08:45 ")?.text, "08:45")
		XCTAssertEqual(DayTime(text: "00:00")?.minuteOfDay, 0)
		XCTAssertEqual(DayTime(text: "23:59")?.text, "23:59")
		for refused in ["", "24:00", "8:60", "8", "8:5", "8:45:00", "ab:cd", "-1:30", "123:00"] {
			XCTAssertNil(DayTime(text: refused), "\(refused) is not a time of day")
		}
		XCTAssertNil(DayTime(minuteOfDay: 1440))
		XCTAssertLessThan(DayTime(minuteOfDay: 60)!, DayTime(minuteOfDay: 61)!)
	}

	func testAFreshModelShowsTheBoardDefaults() throws {
		let rhythm = RhythmModel(session: try OnboardingHarness(self).session)
		XCTAssertEqual(rhythm.time(of: .planning).text, "08:45")
		XCTAssertEqual(rhythm.time(of: .shutdown).text, "17:30")
		XCTAssertEqual(rhythm.time(of: .lightPlanning).text, "18:30")
		XCTAssertEqual(rhythm.day(of: .weeklyReview), .friday)
		XCTAssertEqual(rhythm.time(of: .weeklyReview).text, "16:00")
		XCTAssertEqual(rhythm.day(of: .projectReview), .sunday)
		XCTAssertNil(rhythm.day(of: .planning))
		XCTAssertEqual(rhythm.workingHours.start.text, "09:00")
		XCTAssertEqual(rhythm.workingHours.end.text, "17:30")
		XCTAssertEqual(rhythm.workingHours.days, [.monday, .tuesday, .wednesday, .thursday, .friday])
		XCTAssertEqual(rhythm.breakPattern, .fiftyTen)
		XCTAssertEqual(rhythm.rolloverPosition, .top)
		XCTAssertEqual(rhythm.newTaskPosition, .bottom)
		XCTAssertEqual(rhythm.workloadThresholdMinutes, 480)
		XCTAssertTrue(rhythm.planAfterWorkForLife && rhythm.remindersRing)
	}

	func testEveryChoiceIsWrittenToTheAppDocumentAndSurvivesARelaunch() throws {
		let harness = try OnboardingHarness(self)
		let rhythm = RhythmModel(session: harness.session)
		XCTAssertTrue(rhythm.setTime("9:15", of: .planning))
		XCTAssertTrue(rhythm.setTime("18:00", of: .shutdown))
		XCTAssertTrue(rhythm.setTime("15:30", of: .weeklyReview))
		rhythm.setDay(.thursday, of: .weeklyReview)
		XCTAssertTrue(rhythm.setTime("19:00", of: .lightPlanning))
		XCTAssertTrue(rhythm.setTime("17:45", of: .projectReview))
		rhythm.setDay(.saturday, of: .projectReview)
		rhythm.setWorkingHoursKind(.flexible)
		XCTAssertTrue(rhythm.setWorkingHours(start: "08:00", end: "16:00"))
		rhythm.setWorkDay(.friday, isOn: false)
		rhythm.setWorkDay(.saturday, isOn: true)
		rhythm.setBreakPattern(.twentyFiveFive)
		rhythm.setRolloverPosition(.bottom)
		rhythm.setNewTaskPosition(.top)
		rhythm.setOverrunWarning(.never)
		rhythm.setPlanAfterWorkForLife(false)
		rhythm.setRemindersRing(false)
		XCTAssertTrue(rhythm.setWorkloadThreshold(minutes: 390))
		XCTAssertNil(rhythm.errorMessage)

		let reopened = RhythmModel(session: try harness.relaunched().session)
		XCTAssertEqual(reopened.time(of: .planning).text, "09:15")
		XCTAssertEqual(reopened.time(of: .shutdown).text, "18:00")
		XCTAssertEqual(reopened.time(of: .weeklyReview).text, "15:30")
		XCTAssertEqual(reopened.day(of: .weeklyReview), .thursday)
		XCTAssertEqual(reopened.time(of: .lightPlanning).text, "19:00")
		XCTAssertEqual(reopened.time(of: .projectReview).text, "17:45")
		XCTAssertEqual(reopened.day(of: .projectReview), .saturday)
		XCTAssertEqual(reopened.workingHours, WorkingHoursSetting(kind: .flexible, start: DayTime(text: "08:00")!, end: DayTime(text: "16:00")!, days: [.monday, .tuesday, .wednesday, .thursday, .saturday]))
		XCTAssertEqual(reopened.breakPattern, .twentyFiveFive)
		XCTAssertEqual(reopened.rolloverPosition, .bottom)
		XCTAssertEqual(reopened.newTaskPosition, .top)
		XCTAssertEqual(reopened.overrunWarning, .never)
		XCTAssertFalse(reopened.planAfterWorkForLife)
		XCTAssertFalse(reopened.remindersRing)
		XCTAssertEqual(reopened.workloadThresholdMinutes, 390)
	}

	func testATimeThatIsNotOnTheClockIsRefusedAndNothingIsWritten() throws {
		let harness = try OnboardingHarness(self)
		let rhythm = RhythmModel(session: harness.session)
		XCTAssertFalse(rhythm.setTime("25:00", of: .planning))
		XCTAssertEqual(rhythm.issues["planning"], "Use a time like 08:45")
		XCTAssertEqual(rhythm.time(of: .planning).text, "08:45")
		XCTAssertNil(harness.session.appState.ritualMinute(ritual: "planning"), "nothing was written")
		XCTAssertTrue(rhythm.setTime("08:30", of: .planning))
		XCTAssertNil(rhythm.issues["planning"], "a good value clears the message")
	}

	func testWorkingHoursNeedAnEndAfterTheStartAndADay() throws {
		let harness = try OnboardingHarness(self)
		let rhythm = RhythmModel(session: harness.session)
		XCTAssertFalse(rhythm.setWorkingHours(start: "17:00", end: "09:00"))
		XCTAssertEqual(rhythm.issues["workingHours"], "Work has to end after it starts")
		XCTAssertFalse(rhythm.setWorkingHours(start: "9", end: "17:00"))
		XCTAssertEqual(rhythm.workingHours.start.text, "09:00")
		for day in WeekdayChoice.allCases where day != .monday { rhythm.setWorkDay(day, isOn: false) }
		rhythm.setWorkDay(.monday, isOn: false)
		XCTAssertEqual(rhythm.issues["workingHours"], "Pick at least one working day")
		XCTAssertEqual(rhythm.workingHours.days, [.monday], "the last day cannot be unticked")
		// A flexible day has no fixed end to check.
		rhythm.setWorkingHoursKind(.flexible)
		XCTAssertTrue(rhythm.setWorkingHours(start: "17:00", end: "09:00"))
	}

	func testTheWorkloadThresholdStaysBetweenFifteenMinutesAndADay() throws {
		let rhythm = RhythmModel(session: try OnboardingHarness(self).session)
		XCTAssertFalse(rhythm.setWorkloadThreshold(minutes: 14))
		XCTAssertFalse(rhythm.setWorkloadThreshold(minutes: 1441))
		XCTAssertEqual(rhythm.issues["workload"], "Between 15 minutes and 24 hours")
		XCTAssertEqual(rhythm.workloadThresholdMinutes, 480)
		XCTAssertTrue(rhythm.setWorkloadThreshold(minutes: 15))
		XCTAssertTrue(rhythm.setWorkloadThreshold(minutes: 1440))
	}

	func testOnlyAWeeklyRitualTakesADay() throws {
		let harness = try OnboardingHarness(self)
		let rhythm = RhythmModel(session: harness.session)
		rhythm.setDay(.monday, of: .planning)
		XCTAssertNil(harness.session.appState.ritualDay(ritual: "planning"))
		XCTAssertNil(rhythm.day(of: .planning))
	}
}
