import Foundation
import Observation

/// A provider the step offers.
public enum ProviderChoice: String, CaseIterable, Identifiable, Hashable, Sendable {
	case local
	case openRouter
	case claude
	case openAI
	case gemini
	case other

	public var id: String { rawValue }

	public var title: String {
		switch self {
		case .local: "Local and self-hosted models"
		case .openRouter: "OpenRouter"
		case .claude: "Claude"
		case .openAI: "OpenAI"
		case .gemini: "Google Gemini"
		case .other: "Other provider"
		}
	}

	public var summary: String {
		switch self {
		case .local: "Ollama or LM Studio on this Mac, or a server you run, such as a homelab"
		case .openRouter: "Nearly every model through one account, with auto routing and zero data retention routing"
		case .claude: "Anthropic · Sonnet 5.5 for AI Insight, Haiku 5.5 for AI Glance jobs too heavy for this Mac"
		case .openAI: "GPT models with your own API key"
		case .gemini: "Gemini models with your own API key"
		case .other: "Any OpenAI-compatible endpoint, such as a company gateway"
		}
	}

	/// Whether using the provider sends data off this Mac.
	public var mayLeaveThisMac: Bool { self != .local }

	/// Whether the provider can promise zero data retention.
	public var supportsZeroRetention: Bool { self == .openRouter }

	public var isRecommended: Bool { self == .openRouter }

	/// Whether adding it takes an API key.
	public var needsKey: Bool { self != .local }

	/// Whether adding it takes an endpoint address.
	public var needsEndpoint: Bool { self == .local || self == .other }

	/// The models a provider's key can name for the two levels, `nil` when the step does not ask.
	public var models: [ProviderModel]? {
		self == .claude ? ProviderModel.claude : nil
	}

	var core: ProviderKindView {
		switch self {
		case .local: .local
		case .openRouter: .openRouter
		case .claude: .claude
		case .openAI: .openAi
		case .gemini: .gemini
		case .other: .other
		}
	}

	init(_ view: ProviderKindView) {
		switch view {
		case .local: self = .local
		case .openRouter: self = .openRouter
		case .claude: self = .claude
		case .openAi: self = .openAI
		case .gemini: self = .gemini
		case .other: self = .other
		}
	}
}

/// A model a key can name.
public struct ProviderModel: Identifiable, Hashable, Sendable {
	public let id: String
	public let title: String

	public static let claude = [
		ProviderModel(id: "claude-sonnet-5-5", title: "Claude Sonnet 5.5"),
		ProviderModel(id: "claude-opus-5-5", title: "Claude Opus 5.5"),
		ProviderModel(id: "claude-haiku-5-5", title: "Claude Haiku 5.5"),
	]
}

/// Where a key is used.
public enum ProviderKeyScopeSetting: String, CaseIterable, Identifiable, Hashable, Sendable {
	case `default`
	case perSphere
	case perWorkspace

	public var id: String { rawValue }

	public var title: String {
		switch self {
		case .default: "Default"
		case .perSphere: "Per sphere"
		case .perWorkspace: "Per workspace"
		}
	}

	var core: ProviderKeyScopeView {
		switch self {
		case .default: .default
		case .perSphere: .perSphere
		case .perWorkspace: .perWorkspace
		}
	}

	init(_ view: ProviderKeyScopeView) {
		switch view {
		case .default: self = .default
		case .perSphere: self = .perSphere
		case .perWorkspace: self = .perWorkspace
		}
	}
}

/// A key as the list shows it. There is no field for the key itself: once
/// stored it is never read back to the screen.
public struct ProviderKeyEntry: Identifiable, Hashable, Sendable {
	public let id: String
	public let provider: ProviderChoice
	public let name: String
	public let scope: ProviderKeyScopeSetting
	public let targets: Set<String>
	public let insightModel: String?
	public let glanceModel: String?
	public let endpoint: String?
	/// Whether the Keychain holds the key. A provider that runs locally has none to hold.
	public let isKeyStored: Bool

	/// What the list shows in place of a key.
	public static let mask = "••••••••"
}

/// What the add-a-key sheet collects. The key text lives in the sheet's field
/// and in this value until ``AiProvidersStepModel/add(_:)`` returns.
public struct ProviderKeyDraft: Sendable {
	public var provider: ProviderChoice
	public var name = ""
	public var key = ""
	public var scope: ProviderKeyScopeSetting = .default
	public var targets: Set<String> = []
	public var insightModel: String?
	public var glanceModel: String?
	public var endpoint = ""

	public init(provider: ProviderChoice) {
		self.provider = provider
		insightModel = provider.models?.first?.id
		glanceModel = provider.models?.last?.id
	}
}

/// What adding a key came to.
public enum AddKeyOutcome: Equatable, Sendable {
	case added(id: String)
	/// The fields the draft was refused on, by field name; nothing was stored.
	case refused([String: String])
	/// Storing failed, in words for the user; nothing is left behind.
	case failed(String)
}

/// The AI providers step. A key's secret goes to the Keychain through the
/// `SecretStore` port, on this Mac only; the app-level document records only
/// its name, provider and scope. The base version has no AI, so everything
/// here is inert and says Planned, and no test request is made.
@MainActor
@Observable
public final class AiProvidersStepModel {
	public static let plannedNote = "AI arrives in a later release; keys are kept in the Keychain on this Mac for then"
	/// The line under the key field.
	public static let keyFootnote = "Kept in the Keychain on this Mac, never synced or written to settings.json"

	public private(set) var keys: [ProviderKeyEntry] = []
	public private(set) var retention: DataRetentionSetting = .zeroRetention
	public private(set) var monthlyBudget: Int?
	public private(set) var issues: [String: String] = [:]
	public private(set) var errorMessage: String?

	private let session: CoreSession

	public init(session: CoreSession) {
		self.session = session
		reload()
	}

	/// Reads the key records and the Keychain's list of stored keys.
	public func reload() {
		let appState = session.appState
		retention = DataRetentionSetting(appState.dataRetention())
		monthlyBudget = appState.monthlyCloudBudget().flatMap { Int(exactly: $0) }
		let stored = Set(((try? session.environment.secrets.list(kind: .providerApiKey)) ?? []).map(\.name))
		keys = appState.providerKeys().map { record in
			let provider = ProviderChoice(record.provider)
			return ProviderKeyEntry(
				id: record.id,
				provider: provider,
				name: record.name,
				scope: ProviderKeyScopeSetting(record.scope),
				targets: Set(record.targets),
				insightModel: record.insightModel,
				glanceModel: record.glanceModel,
				endpoint: record.endpoint,
				isKeyStored: provider.needsKey ? stored.contains(record.id) : false,
			)
		}
	}

	public func keys(of provider: ProviderChoice) -> [ProviderKeyEntry] {
		keys.filter { $0.provider == provider }
	}

	/// Whether a provider is paused: a cloud provider that cannot promise what the data retention choice asks for.
	public func isPaused(_ provider: ProviderChoice) -> Bool {
		provider.mayLeaveThisMac && retention == .zeroRetention && !provider.supportsZeroRetention
	}

	/// Checks a draft, stores its key in the Keychain, then records it. A key whose record could not be written is deleted again.
	@discardableResult
	public func add(_ draft: ProviderKeyDraft) -> AddKeyOutcome {
		let problems = Self.problems(in: draft)
		issues = problems
		guard problems.isEmpty else { return .refused(problems) }
		let id = "key-\(UUID().uuidString.prefix(8).lowercased())"
		let provider = draft.provider
		let secrets = session.environment.secrets
		let secretId = SecretId(kind: .providerApiKey, name: id)
		do {
			if provider.needsKey {
				let item = SecretItem(id: secretId, isSynchronisable: false, value: Data(draft.key.trimmingCharacters(in: .whitespacesAndNewlines).utf8))
				try secrets.put(item: item)
			}
			let record = ProviderKeyView(
				id: id,
				provider: provider.core,
				name: draft.name.trimmingCharacters(in: .whitespaces),
				scope: draft.scope.core,
				targets: draft.scope == .default ? [] : draft.targets.sorted(),
				insightModel: provider.models == nil ? nil : draft.insightModel,
				glanceModel: provider.models == nil ? nil : draft.glanceModel,
				endpoint: provider.needsEndpoint ? draft.endpoint.trimmingCharacters(in: .whitespaces) : nil,
			)
			do {
				try session.edit { try $0.putProviderKey(key: record) }
			} catch {
				if provider.needsKey { try? secrets.delete(id: secretId) }
				throw error
			}
		} catch {
			let message = coreErrorMessage(error)
			errorMessage = message
			return .failed(message)
		}
		errorMessage = nil
		reload()
		return .added(id: id)
	}

	/// Removes a key: its record and its secret.
	public func remove(id: String) {
		do {
			try session.edit { try $0.removeProviderKey(id: id) }
			try session.environment.secrets.delete(id: SecretId(kind: .providerApiKey, name: id))
			errorMessage = nil
		} catch {
			errorMessage = coreErrorMessage(error)
		}
		reload()
	}

	public func setRetention(_ retention: DataRetentionSetting) {
		do {
			try session.edit { try $0.setDataRetention(retention: retention.core) }
			errorMessage = nil
		} catch {
			errorMessage = coreErrorMessage(error)
		}
		reload()
	}

	/// Sets the monthly cloud budget from the text of its field; empty clears it, anything but a whole number of zero or more is refused.
	@discardableResult
	public func setMonthlyBudget(_ text: String) -> Bool {
		let trimmed = text.trimmingCharacters(in: .whitespaces)
		let budget: UInt64?
		if trimmed.isEmpty {
			budget = nil
		} else if let value = UInt64(trimmed) {
			budget = value
		} else {
			issues["budget"] = "Use a whole number, or leave it empty for no limit"
			return false
		}
		issues["budget"] = nil
		do {
			try session.edit { try $0.setMonthlyCloudBudget(budget: budget) }
			errorMessage = nil
		} catch {
			errorMessage = coreErrorMessage(error)
			return false
		}
		reload()
		return true
	}

	static func problems(in draft: ProviderKeyDraft) -> [String: String] {
		var problems: [String: String] = [:]
		let name = draft.name.trimmingCharacters(in: .whitespaces)
		if name.isEmpty { problems["name"] = "Name the key" }
		if name.count > 60 { problems["name"] = "Use at most 60 characters" }
		if draft.provider.needsKey {
			let key = draft.key.trimmingCharacters(in: .whitespacesAndNewlines)
			if key.isEmpty {
				problems["key"] = "Paste the API key"
			} else if key.rangeOfCharacter(from: .whitespacesAndNewlines) != nil {
				problems["key"] = "A key has no spaces or line breaks"
			}
		}
		if draft.provider.needsEndpoint {
			let endpoint = draft.endpoint.trimmingCharacters(in: .whitespaces)
			let url = URL(string: endpoint)
			if url?.host() == nil || !["http", "https"].contains(url?.scheme ?? "") {
				problems["endpoint"] = "Use an address like http://localhost:11434"
			}
		}
		if draft.scope != .default, draft.targets.isEmpty {
			problems["scope"] = draft.scope == .perSphere ? "Pick at least one sphere" : "Pick at least one workspace"
		}
		return problems
	}
}
