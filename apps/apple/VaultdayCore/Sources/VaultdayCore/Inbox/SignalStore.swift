import AppKit
import Foundation

/// What the Inbox needs from a workspace's signal store. The core's handles
/// implement it (``CoreSignalStore``); tests hand in synthetic signals.
@MainActor
public protocol SignalStore: AnyObject {
	func inboxSignals() throws -> [InboxSignalView]
	/// Writes one task line from a signal, once.
	func convert(_ input: SignalConversionInput) throws -> SignalConversionView
	func attach(signalId: String, story: String) throws
	/// `until` is RFC 3339.
	func snooze(signalId: String, until: String) throws
	func dismiss(signalId: String) throws
	func reject(signalId: String) throws
	func storyChoices() -> [StoryChoice]
	/// Opens the message in its account's mail app.
	func openInMailApp(signalId: String) -> Bool
}

/// The open workspace's signal store in the core.
@MainActor
public final class CoreSignalStore: SignalStore {
	private let workspace: WorkspaceModel

	public init(workspace: WorkspaceModel) {
		self.workspace = workspace
	}

	public func inboxSignals() throws -> [InboxSignalView] {
		let (vault, state) = try handles()
		return try state.inboxSignals(workspace: vault)
	}

	public func convert(_ input: SignalConversionInput) throws -> SignalConversionView {
		let (vault, state) = try handles()
		let converted = try state.convertSignal(workspace: vault, input: input)
		_ = try state.flush()
		return converted
	}

	public func attach(signalId: String, story: String) throws {
		let (vault, state) = try handles()
		_ = try state.attachSignal(workspace: vault, id: signalId, story: story)
		_ = try state.flush()
	}

	public func snooze(signalId: String, until: String) throws {
		let (_, state) = try handles()
		_ = try state.snoozeSignal(id: signalId, until: until)
		_ = try state.flush()
	}

	public func dismiss(signalId: String) throws {
		let (_, state) = try handles()
		_ = try state.dismissSignal(id: signalId)
		_ = try state.flush()
	}

	public func reject(signalId: String) throws {
		let (_, state) = try handles()
		_ = try state.rejectSignal(id: signalId)
		_ = try state.flush()
	}

	public func storyChoices() -> [StoryChoice] {
		guard let vault = workspace.vault else { return [] }
		return ((try? VaultdayCore.storyChoices(workspace: vault)) ?? []).map { StoryChoice(number: $0.number, title: $0.title) }
	}

	public func openInMailApp(signalId: String) -> Bool {
		SignalLinks.open(signalId: signalId, in: workspace)
	}

	private func handles() throws -> (WorkspaceHandle, WorkspaceStateHandle) {
		guard let vault = workspace.vault, let state = workspace.state else { throw SignalStoreClosed() }
		return (vault, state)
	}
}

/// No workspace is open.
struct SignalStoreClosed: LocalizedError {
	var errorDescription: String? { "Open a workspace first" }
}
