import Foundation
import Security

/// The localhost API's bearer token: random, made by the app on its first
/// run, kept in the Keychain through the `SecretStore` port (this device
/// only), and written mode 0600 into the App Group container where the CLI
/// reads it: `~/Library/Group Containers/group.com.eiluviann.vaultday/API/token`,
/// beside `API/port` with the port the server listens on.
public enum LocalAPICredentials {
	/// The Keychain item's name within ``SecretKind/localApiToken``.
	public static let secretName = "local-api"
	/// 32 random bytes, written as 64 lowercase hex digits.
	static let tokenByteCount = 32

	/// The token from the Keychain, made and stored there first when this
	/// Mac has none, then written to the token file.
	public static func prepareToken(secrets: any SecretStorePort, layout: AppGroupLayout) throws -> String {
		let token = try storedOrNewToken(secrets: secrets)
		try writeOwnerOnly(Data((token + "\n").utf8), to: layout.apiTokenFile)
		return token
	}

	/// Writes the port the server listens on, for the CLI.
	public static func writePort(_ port: UInt16, layout: AppGroupLayout) throws {
		try writeOwnerOnly(Data("\(port)\n".utf8), to: layout.apiPortFile)
	}

	/// Removes the port file when the server stops, so the CLI reads "not running" at once.
	public static func removePort(layout: AppGroupLayout) {
		try? FileManager.default.removeItem(at: layout.apiPortFile)
	}

	private static func storedOrNewToken(secrets: any SecretStorePort) throws -> String {
		let id = SecretId(kind: .localApiToken, name: secretName)
		if let item = try secrets.get(id: id), let token = String(data: item.value, encoding: .utf8), isWellFormed(token) {
			return token
		}
		let token = try randomToken()
		let item = SecretItem(id: id, isSynchronisable: secretKindIsSynchronisable(kind: .localApiToken), value: Data(token.utf8))
		try secrets.put(item: item)
		return token
	}

	static func randomToken() throws -> String {
		var bytes = [UInt8](repeating: 0, count: tokenByteCount)
		let status = SecRandomCopyBytes(kSecRandomDefault, bytes.count, &bytes)
		guard status == errSecSuccess else { throw LocalAPIError.randomUnavailable(status) }
		return bytes.map { String(format: "%02x", $0) }.joined()
	}

	static func isWellFormed(_ token: String) -> Bool {
		token.count == tokenByteCount * 2 && token.allSatisfy(\.isHexDigit)
	}

	/// Writes `data` so that no moment exists where the file is readable by
	/// others: the folder is 0700, the temporary file is created 0600, then
	/// moved over the old one.
	static func writeOwnerOnly(_ data: Data, to url: URL) throws {
		let folder = url.deletingLastPathComponent()
		let manager = FileManager.default
		try manager.createDirectory(at: folder, withIntermediateDirectories: true, attributes: [.posixPermissions: 0o700])
		try manager.setAttributes([.posixPermissions: 0o700], ofItemAtPath: folder.path)
		let temporary = folder.appending(path: ".\(url.lastPathComponent).\(UUID().uuidString)")
		guard manager.createFile(atPath: temporary.path, contents: data, attributes: [.posixPermissions: 0o600]) else {
			throw LocalAPIError.fileNotWritable(temporary.path)
		}
		do {
			if manager.fileExists(atPath: url.path) {
				_ = try manager.replaceItemAt(url, withItemAt: temporary)
			} else {
				try manager.moveItem(at: temporary, to: url)
			}
			try manager.setAttributes([.posixPermissions: 0o600], ofItemAtPath: url.path)
		} catch {
			try? manager.removeItem(at: temporary)
			throw error
		}
	}
}

/// Why the localhost API could not start.
public enum LocalAPIError: Error, LocalizedError, Equatable {
	case randomUnavailable(OSStatus)
	case fileNotWritable(String)
	case listenerFailed(String)

	public var errorDescription: String? {
		switch self {
		case let .randomUnavailable(status): "No random bytes for the API token (\(status))"
		case let .fileNotWritable(path): "Could not write \(path)"
		case let .listenerFailed(reason): "The localhost API could not listen: \(reason)"
		}
	}
}
