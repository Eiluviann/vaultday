import Foundation

/// The twelve steps of the setup window, in order.
public enum SetupStep: Int, CaseIterable, Identifiable, Sendable {
	case welcome = 1
	case workspaces
	case folders
	case syncAndBackup
	case accounts
	case featureSets
	case security
	case rituals
	case aiLevel
	case aiProviders
	case aiAssistance
	case ready

	public var id: Int { rawValue }

	/// The title in the StepList.
	public var title: String {
		switch self {
		case .welcome: "Welcome"
		case .workspaces: "Workspaces"
		case .folders: "Folders"
		case .syncAndBackup: "Sync and backup"
		case .accounts: "Accounts"
		case .featureSets: "Feature sets"
		case .security: "Security"
		case .rituals: "Rituals and routines"
		case .aiLevel: "AI level"
		case .aiProviders: "AI providers"
		case .aiAssistance: "AI assistance"
		case .ready: "Ready"
		}
	}

	/// The step after this one, if any.
	public var next: SetupStep? { SetupStep(rawValue: rawValue + 1) }

	/// The step before this one, if any.
	public var previous: SetupStep? { SetupStep(rawValue: rawValue - 1) }
}
