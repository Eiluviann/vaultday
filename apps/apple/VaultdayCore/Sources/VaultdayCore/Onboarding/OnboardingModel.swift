import Foundation
import Observation

/// Where a restore starts looking.
public enum RestoreSource: Sendable {
	case iCloud
	case localFolder
}

/// How a step shows in the StepList.
public enum StepMark: Equatable, Sendable {
	case done
	case current
	case upcoming
}

/// The setup window: which of the twelve steps is shown, how far the user has
/// been, and Back, Continue and Restore. The step is kept in the app-level
/// document, so a relaunch returns to it.
@MainActor
@Observable
public final class OnboardingModel {
	public private(set) var current: SetupStep
	public private(set) var furthest: SetupStep
	public private(set) var isFinished: Bool
	/// The result of the last restore, for its popover.
	public private(set) var restoreMessage: String?
	public private(set) var errorMessage: String?

	private let session: CoreSession
	private let workspaces: WorkspacesModel

	public init(session: CoreSession, workspaces: WorkspacesModel) {
		self.session = session
		self.workspaces = workspaces
		let progress = session.appState.onboardingProgress()
		current = SetupStep(rawValue: Int(progress.currentStep)) ?? .welcome
		furthest = SetupStep(rawValue: Int(progress.furthestStep)) ?? .welcome
		isFinished = progress.isFinished
	}

	public var canGoBack: Bool { current.previous != nil }
	public var canContinue: Bool { current.next != nil }
	/// Restore takes Back's place on the first step.
	public var isRestoreAvailable: Bool { current == .welcome }
	public var continueTitle: String { current == .welcome ? "Get started" : "Continue" }
	public var progressText: String { "Step \(current.rawValue) of \(SetupStep.allCases.count)" }

	/// How a step shows: steps already reached are links back, the shown one is raised.
	public func mark(of step: SetupStep) -> StepMark {
		if step == current { return .current }
		return step.rawValue <= furthest.rawValue ? .done : .upcoming
	}

	/// Moves to the next step.
	public func goForward() {
		guard let next = current.next else { return }
		show(next)
	}

	/// Moves to the previous step.
	public func goBack() {
		guard let previous = current.previous else { return }
		show(previous)
	}

	/// Jumps to a step already reached; a step not reached yet is refused.
	@discardableResult
	public func jump(to step: SetupStep) -> Bool {
		guard step.rawValue <= furthest.rawValue else { return false }
		show(step)
		return true
	}

	/// Restores workspaces from a folder and, when it found any, shows the
	/// Workspaces step.
	@discardableResult
	public func restore(from folder: URL) -> RestoreOutcome {
		let outcome = workspaces.restore(from: folder)
		restoreMessage = outcome.message
		if case .restored = outcome {
			show(.workspaces)
		}
		return outcome
	}

	private func show(_ step: SetupStep) {
		current = step
		furthest = SetupStep(rawValue: max(furthest.rawValue, step.rawValue)) ?? furthest
		do {
			try session.edit { try $0.setOnboardingStep(step: UInt32(step.rawValue)) }
			errorMessage = nil
		} catch {
			errorMessage = coreErrorMessage(error)
		}
	}
}
