import CryptoKit
import Foundation

/// Lets the command line check that the program on the API port is Vaultday
/// before it sends the token. `GET /v1/hello` with a fresh random challenge
/// answers the HMAC-SHA256 of it keyed by the token; a program squatting on
/// a stale port (another account on the Mac, say) cannot make it, so the
/// command line never hands it the token.
public enum ServerProof {
	public static let route = "/v1/hello"
	/// The request header holding the challenge, lowercased like every parsed header.
	public static let challengeHeader = "x-vaultday-challenge"
	static let context = "vaultday-server-proof\n"

	/// The proof for `challenge`, as 64 lowercase hex digits.
	public static func proof(challenge: String, token: String) -> String {
		let key = SymmetricKey(data: Data(token.utf8))
		let code = HMAC<SHA256>.authenticationCode(for: Data((context + challenge).utf8), using: key)
		return code.map { String(format: "%02x", $0) }.joined()
	}

	/// 32 to 128 hex digits: enough entropy, and nothing else to sign.
	public static func isWellFormedChallenge(_ challenge: String) -> Bool {
		(32 ... 128).contains(challenge.count) && challenge.allSatisfy(\.isHexDigit)
	}
}
