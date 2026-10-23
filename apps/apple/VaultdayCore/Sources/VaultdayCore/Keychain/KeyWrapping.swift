import CryptoKit
import Foundation

/// Wraps key material for storage at rest on this Mac.
public protocol KeyWrapping: Sendable {
	/// Whether the wrapping key lives in the Secure Enclave.
	var isHardwareBacked: Bool { get }
	/// Seals `secret`, bound to `context` (the item's address).
	func wrap(_ secret: Data, context: Data) throws -> Data
	/// Opens a blob `wrap` produced for the same `context`.
	func unwrap(_ blob: Data, context: Data) throws -> Data
}

/// Why a wrapped blob could not be opened.
public enum KeyWrappingError: Error, Equatable, Sendable {
	/// The blob is not in the wrapping format.
	case malformed
	/// The blob was sealed by another key or for another item.
	case notOpenable
}

/// ECIES over P-256: an ephemeral key agreement with the wrapping key's public
/// half, HKDF-SHA256 into an AES-GCM key, the item's address as associated
/// data. With a Secure Enclave key, unwrapping needs this Mac's enclave.
///
/// Blob: version `0x01`, the ephemeral public key (65 bytes, X9.63), then
/// the AES-GCM combined box (nonce, ciphertext, tag). The version byte and
/// the item's address are the associated data.
public struct ECIESKeyWrapper: KeyWrapping {
	public typealias KeyAgreement = @Sendable (P256.KeyAgreement.PublicKey) throws -> SharedSecret

	private static let version: UInt8 = 1
	private static let ephemeralKeyLength = 65
	private static let info = Data("vaultday key wrap v1".utf8)

	public let isHardwareBacked: Bool
	private let publicKey: P256.KeyAgreement.PublicKey
	private let agree: KeyAgreement

	public init(publicKey: P256.KeyAgreement.PublicKey, isHardwareBacked: Bool, agree: @escaping KeyAgreement) {
		self.publicKey = publicKey
		self.isHardwareBacked = isHardwareBacked
		self.agree = agree
	}

	/// A wrapper over a software key, for tests and the CI runners, which
	/// have no Secure Enclave.
	public static func software(_ privateKey: P256.KeyAgreement.PrivateKey = .init()) -> ECIESKeyWrapper {
		ECIESKeyWrapper(publicKey: privateKey.publicKey, isHardwareBacked: false) {
			try privateKey.sharedSecretFromKeyAgreement(with: $0)
		}
	}

	/// A wrapper over a software key kept device-only in the Keychain at
	/// `address`, standing in for the enclave where tests need one that
	/// survives a relaunch.
	public static func software(backend: any KeychainBackend, address: KeychainItemAddress) throws -> ECIESKeyWrapper {
		if let stored = try backend.data(at: address),
		   let key = try? P256.KeyAgreement.PrivateKey(rawRepresentation: stored) {
			return software(key)
		}
		let key = P256.KeyAgreement.PrivateKey()
		try backend.upsert(key.rawRepresentation, at: address, accessibility: .afterFirstUnlockThisDeviceOnly)
		return software(key)
	}

	/// A wrapper over this Mac's Secure Enclave key: the enclave-bound key
	/// blob is kept device-only in the Keychain; a blob that no longer opens
	/// (a reset enclave) is replaced by a new key. Data keys the old key
	/// sealed are re-read from their synchronised items; a device-only item it
	/// sealed (the signing seed) reads as missing, so the core mints a new
	/// device identity.
	public static func secureEnclave(backend: any KeychainBackend, address: KeychainItemAddress) throws -> ECIESKeyWrapper {
		let privateKey = try loadOrCreateEnclaveKey(backend: backend, address: address)
		let box = EnclaveKeyBox(key: privateKey)
		return ECIESKeyWrapper(publicKey: privateKey.publicKey, isHardwareBacked: true) {
			try box.key.sharedSecretFromKeyAgreement(with: $0)
		}
	}

	public func wrap(_ secret: Data, context: Data) throws -> Data {
		let ephemeral = P256.KeyAgreement.PrivateKey()
		let ephemeralPublic = ephemeral.publicKey.x963Representation
		let shared = try ephemeral.sharedSecretFromKeyAgreement(with: publicKey)
		let key = symmetricKey(from: shared, ephemeralPublic: ephemeralPublic)
		let sealed = try AES.GCM.seal(secret, using: key, authenticating: Self.associatedData(context))
		guard let combined = sealed.combined else { throw KeyWrappingError.malformed }
		return Data([Self.version]) + ephemeralPublic + combined
	}

	public func unwrap(_ blob: Data, context: Data) throws -> Data {
		let bytes = [UInt8](blob)
		guard bytes.count > 1 + Self.ephemeralKeyLength, bytes[0] == Self.version else {
			throw KeyWrappingError.malformed
		}
		let ephemeralPublic = Data(bytes[1 ... Self.ephemeralKeyLength])
		let combined = Data(bytes[(1 + Self.ephemeralKeyLength)...])
		do {
			let ephemeral = try P256.KeyAgreement.PublicKey(x963Representation: ephemeralPublic)
			let key = symmetricKey(from: try agree(ephemeral), ephemeralPublic: ephemeralPublic)
			return try AES.GCM.open(AES.GCM.SealedBox(combined: combined), using: key, authenticating: Self.associatedData(context))
		} catch {
			throw KeyWrappingError.notOpenable
		}
	}

	/// The version byte and the item's address, both authenticated.
	private static func associatedData(_ context: Data) -> Data {
		Data([version]) + context
	}

	private func symmetricKey(from shared: SharedSecret, ephemeralPublic: Data) -> SymmetricKey {
		shared.hkdfDerivedSymmetricKey(
			using: SHA256.self,
			salt: ephemeralPublic + publicKey.x963Representation,
			sharedInfo: Self.info,
			outputByteCount: 32,
		)
	}

	private static func loadOrCreateEnclaveKey(
		backend: any KeychainBackend,
		address: KeychainItemAddress,
	) throws -> SecureEnclave.P256.KeyAgreement.PrivateKey {
		if let stored = try backend.data(at: address),
		   let key = try? SecureEnclave.P256.KeyAgreement.PrivateKey(dataRepresentation: stored) {
			return key
		}
		let key = try SecureEnclave.P256.KeyAgreement.PrivateKey()
		try backend.upsert(key.dataRepresentation, at: address, accessibility: .afterFirstUnlockThisDeviceOnly)
		return key
	}
}

// The enclave key is an immutable handle; CryptoKit does not mark it Sendable.
private final class EnclaveKeyBox: @unchecked Sendable {
	let key: SecureEnclave.P256.KeyAgreement.PrivateKey

	init(key: SecureEnclave.P256.KeyAgreement.PrivateKey) {
		self.key = key
	}
}
