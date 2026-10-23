import Foundation
import Observation

/// A workspace's billing model, as the Configure sheet offers it.
public enum BillingModelChoice: String, CaseIterable, Hashable, Sendable {
	case none
	case hourly
	case fixed
	case fte

	public var title: String {
		switch self {
		case .none: "None"
		case .hourly: "Hourly"
		case .fixed: "Fixed price"
		case .fte: "FTE"
		}
	}

	/// What the model does, one line for under the picker.
	public var note: String {
		switch self {
		case .none: "Sessions stay in Vaultday"
		case .hourly: "Sessions are billable time, rounded up per workspace"
		case .fixed: "The estimate is the quote and the cap; an overrun is unpaid"
		case .fte: "Tracked time stays private; Toggl gets a flat day"
		}
	}
}

/// The `billing` block of a workspace's `settings.json` and its hourly rate,
/// which lives in the signed app document, never in the file. Saved
/// together from the Configure sheet's Hours tab.
@MainActor
@Observable
public final class BillingSettingsModel {
	public let root: String
	public let workspaceId: String
	public var model: BillingModelChoice = .none
	public var roundingMinutes = 15
	public var isBillableByDefault = true
	public var flatHours = "8"
	/// The hourly rate in the currency's main unit, `"95"` or `"95.50"`; blank for none.
	public var rate = ""
	public var currency = "EUR"
	public private(set) var errorMessage: String?

	private let session: CoreSession?

	public init(root: String, workspaceId: String, session: CoreSession?) {
		self.root = root
		self.workspaceId = workspaceId
		self.session = session
		reload()
	}

	public static let roundingChoices = [1, 5, 6, 10, 15, 30, 60]

	public func reload() {
		do {
			let block = try readWorkspaceBilling(root: root)
			model = switch block.model {
			case .hourly: .hourly
			case .fixed: .fixed
			case .fte: .fte
			case nil: .none
			}
			roundingMinutes = Int(block.roundingMinutes)
			isBillableByDefault = block.isBillableByDefault
			flatHours = Self.hoursText(block.flatHours)
			if let stored = session?.appState.billingRate(workspace: workspaceId) {
				rate = Self.amountText(minor: stored.amountMinorPerHour)
				currency = stored.currency
			}
			errorMessage = nil
		} catch {
			errorMessage = coreErrorMessage(error)
		}
	}

	/// Writes the block (only the chosen model's keys) and the rate.
	@discardableResult
	public func save() -> Bool {
		guard let hours = Double(flatHours.replacingOccurrences(of: ",", with: ".")) else {
			errorMessage = "Flat hours is a number such as 8 or 7.5"
			return false
		}
		let minor: UInt64?
		let trimmedRate = rate.trimmingCharacters(in: .whitespaces)
		if trimmedRate.isEmpty {
			minor = nil
		} else if let amount = Decimal(string: trimmedRate.replacingOccurrences(of: ",", with: ".")), amount >= 0 {
			minor = NSDecimalNumber(decimal: amount * 100).uint64Value
		} else {
			errorMessage = "The rate is an amount such as 95 or 95.50"
			return false
		}
		let view = BillingSettingsView(
			model: Self.coreModel(model),
			roundingMinutes: UInt32(clamping: roundingMinutes),
			isBillableByDefault: isBillableByDefault,
			flatHours: hours,
		)
		do {
			_ = try updateWorkspaceBilling(root: root, billing: view)
			if let session {
				try session.edit { handle in
					try handle.setBillingRate(
						workspace: workspaceId,
						rate: minor.map { BillingRateView(amountMinorPerHour: $0, currency: currency) },
					)
				}
			}
			errorMessage = nil
			reload()
			return true
		} catch {
			errorMessage = coreErrorMessage(error)
			return false
		}
	}

	private static func coreModel(_ choice: BillingModelChoice) -> BillingModelView? {
		switch choice {
		case .none: nil
		case .hourly: .hourly
		case .fixed: .fixed
		case .fte: .fte
		}
	}

	private static func hoursText(_ hours: Double) -> String {
		hours == hours.rounded() ? String(Int(hours)) : String(hours)
	}

	private static func amountText(minor: UInt64) -> String {
		minor % 100 == 0 ? String(minor / 100) : String(format: "%llu.%02llu", minor / 100, minor % 100)
	}
}
