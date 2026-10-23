import Foundation

/// Compares a presented token with the expected one in time that depends
/// only on the expected token's length, never on where the first differing
/// byte sits, so the comparison leaks nothing about the token.
public enum TokenComparison {
	public static func isEqual(_ presented: String, _ expected: String) -> Bool {
		let presentedBytes = Array(presented.utf8)
		let expectedBytes = Array(expected.utf8)
		var difference = UInt8(truncatingIfNeeded: presentedBytes.count ^ expectedBytes.count)
		difference |= presentedBytes.count == expectedBytes.count ? 0 : 1
		for index in expectedBytes.indices {
			let presentedByte = index < presentedBytes.count ? presentedBytes[index] : 0
			difference |= presentedByte ^ expectedBytes[index]
		}
		return difference == 0 && !expectedBytes.isEmpty
	}

	/// The token of an `Authorization: Bearer <token>` header, `nil` for any other scheme.
	public static func bearerToken(_ header: String?) -> String? {
		guard let header else { return nil }
		let parts = header.split(separator: " ", maxSplits: 1)
		guard parts.count == 2, parts[0].lowercased() == "bearer" else { return nil }
		return parts[1].trimmingCharacters(in: .whitespaces)
	}
}
