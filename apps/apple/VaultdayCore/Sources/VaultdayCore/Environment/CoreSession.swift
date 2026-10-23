import Foundation

/// The app-level document opened over a ``CoreEnvironment``: one per app
/// run, shared by the models. Opening it verifies every device's signed
/// snapshot; refused ones are listed for the user.
@MainActor
public final class CoreSession {
	public let environment: CoreEnvironment
	/// This device's id.
	public let deviceId: String
	/// Snapshots that failed verification at open, in words for the user.
	public private(set) var refusedSnapshots: [String]
	let appState: AppStateHandle

	private init(environment: CoreEnvironment, appState: AppStateHandle) {
		self.environment = environment
		self.appState = appState
		deviceId = appState.deviceId()
		refusedSnapshots = appState.lastLoad().refused.map { "\($0.path): \($0.reason)" }
	}

	/// Opens the app-level document in the App Group container.
	///
	/// - Throws: ``CoreSessionError`` with a message for the user.
	public static func open(_ environment: CoreEnvironment) throws(CoreSessionError) -> CoreSession {
		do {
			let handle = try openAppState(
				files: environment.files,
				dir: environment.layout.appStateFolder.path,
				secrets: environment.secrets,
			)
			return CoreSession(environment: environment, appState: handle)
		} catch {
			throw CoreSessionError(message: coreErrorMessage(error))
		}
	}

	/// Writes this device's signed snapshot after an edit.
	func save() throws {
		_ = try appState.save()
	}

	/// Runs an edit of the app-level document and writes the snapshot.
	func edit(_ body: (AppStateHandle) throws -> Void) throws {
		try body(appState)
		try save()
	}
}

/// The app-level document could not be opened.
public struct CoreSessionError: Error, Equatable, LocalizedError {
	public let message: String
	public var errorDescription: String? { message }
}

/// The message of an error from the core or a port, for the user.
func coreErrorMessage(_ error: any Error) -> String {
	switch error {
	case let error as CoreError:
		switch error {
		case let .NotAWorkspace(message), let .InvalidSettings(message), let .FileAccess(message),
		     let .InvalidDate(message), let .InvalidInput(message), let .NotFound(message),
		     let .Conflict(message), let .SecretStore(message), let .KeyNotArrived(message),
		     let .AppState(message):
			message
		}
	case let error as LocalizedError:
		error.errorDescription ?? String(describing: error)
	default:
		(error as NSError).localizedDescription
	}
}
