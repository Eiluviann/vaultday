import CryptoKit
import Darwin
import Foundation
import Network
import XCTest
@testable import VaultdayCore

/// The localhost API: the token file, the loopback-only listener, bearer
/// auth, the body limit, and every route against a fake backend, exercised
/// with curl against a server on an ephemeral port.
@MainActor
final class LocalAPITests: XCTestCase {
	private static let token = String(repeating: "ab", count: 32)

	// MARK: Token

	func testTheTokenIsMadeOnceKeptInTheKeychainAndWrittenOwnerOnly() throws {
		let container = try TestFixtures.temporaryFolder(self, "Group")
		let environment = try CoreEnvironment.inMemory(container: container)
		let layout = AppGroupLayout(container: container)

		let token = try LocalAPICredentials.prepareToken(secrets: environment.secrets, layout: layout)
		XCTAssertTrue(LocalAPICredentials.isWellFormed(token), token)
		XCTAssertEqual(layout.apiTokenFile.path, container.appending(path: "API/token").path)
		XCTAssertEqual(try String(contentsOf: layout.apiTokenFile, encoding: .utf8), token + "\n")
		XCTAssertEqual(try permissions(of: layout.apiTokenFile), 0o600)
		XCTAssertEqual(try permissions(of: layout.apiTokenFile.deletingLastPathComponent()), 0o700)

		let stored = try XCTUnwrap(environment.secrets.get(id: SecretId(kind: .localApiToken, name: LocalAPICredentials.secretName)))
		XCTAssertEqual(String(data: stored.value, encoding: .utf8), token)
		XCTAssertFalse(stored.isSynchronisable, "the token stays on this Mac")

		try FileManager.default.removeItem(at: layout.apiTokenFile)
		XCTAssertEqual(try LocalAPICredentials.prepareToken(secrets: environment.secrets, layout: layout), token, "the next launch reuses the Keychain's token")
		XCTAssertEqual(try permissions(of: layout.apiTokenFile), 0o600)

		try LocalAPICredentials.writePort(47390, layout: layout)
		XCTAssertEqual(try String(contentsOf: layout.apiPortFile, encoding: .utf8), "47390\n")
		XCTAssertEqual(try permissions(of: layout.apiPortFile), 0o600)
		LocalAPICredentials.removePort(layout: layout)
		XCTAssertFalse(FileManager.default.fileExists(atPath: layout.apiPortFile.path))
	}

	func testTokensAreComparedWholeAndOnlyAsBearer() {
		XCTAssertTrue(TokenComparison.isEqual(Self.token, Self.token))
		XCTAssertFalse(TokenComparison.isEqual(String(Self.token.dropLast()), Self.token))
		XCTAssertFalse(TokenComparison.isEqual(Self.token + "a", Self.token))
		XCTAssertFalse(TokenComparison.isEqual("", ""))
		XCTAssertFalse(TokenComparison.isEqual("ab" + String(Self.token.dropFirst(2)).uppercased(), Self.token))
		XCTAssertEqual(TokenComparison.bearerToken("Bearer abc"), "abc")
		XCTAssertEqual(TokenComparison.bearerToken("bearer abc"), "abc")
		XCTAssertNil(TokenComparison.bearerToken("Basic abc"))
		XCTAssertNil(TokenComparison.bearerToken(nil))
	}

	// MARK: Loopback

	func testOnlyLoopbackPeersCount() {
		XCTAssertTrue(LocalAPIServer.isLoopback(.hostPort(host: "127.0.0.1", port: 50000)))
		XCTAssertTrue(LocalAPIServer.isLoopback(.hostPort(host: "127.0.0.2", port: 50000)))
		XCTAssertTrue(LocalAPIServer.isLoopback(.hostPort(host: "::1", port: 50000)))
		XCTAssertFalse(LocalAPIServer.isLoopback(.hostPort(host: "192.168.1.5", port: 50000)))
		XCTAssertFalse(LocalAPIServer.isLoopback(.hostPort(host: "10.0.0.2", port: 50000)))
		XCTAssertFalse(LocalAPIServer.isLoopback(.hostPort(host: "fe80::1", port: 50000)))
		XCTAssertFalse(LocalAPIServer.isLoopback(.service(name: "x", type: "_http._tcp", domain: "local", interface: nil)))
	}

	func testANonLoopbackPeerIsRefusedEvenWithTheToken() {
		let backend = FakeTimerBackend()
		let router = TimerAPIRouter(token: Self.token, backend: backend)
		let request = HTTPRequest(method: "GET", path: "/v1/status", headers: ["Authorization": "Bearer \(Self.token)"])
		XCTAssertEqual(router.respond(to: request, peerIsLoopback: false).status, 403)
		XCTAssertEqual(router.respond(to: request, peerIsLoopback: true).status, 200)
		XCTAssertEqual(backend.calls, ["status"], "the refused request never reached the backend")
	}

	func testTheListenerIsNotReachableOnTheMacsNetworkAddress() async throws {
		let address = try XCTUnwrap(Self.nonLoopbackIPv4Address(), "no network interface to try")
		let backend = FakeTimerBackend()
		let (server, port) = try await startServer(backend: backend)
		defer { server.stop() }
		let local = try await curl(port: port, path: "/v1/status", token: Self.token)
		XCTAssertEqual(local.status, 200)
		let remote = try await curl(port: port, path: "/v1/status", token: Self.token, host: address)
		XCTAssertEqual(remote.status, 0, "curl reaches nothing on \(address):\(port)")
		XCTAssertEqual(backend.calls, ["status"])
	}

	// MARK: Auth and limits

	func testHelloProvesTheServerHoldsTheTokenWithoutReceivingIt() async throws {
		let backend = FakeTimerBackend()
		let (server, port) = try await startServer(backend: backend)
		defer { server.stop() }
		let challenge = String(repeating: "ab", count: 32)
		let hello = try await curl(port: port, path: "/v1/hello", token: nil, extraHeaders: ["X-Vaultday-Challenge: \(challenge)"])
		XCTAssertEqual(hello.status, 200)
		let answer = try JSONDecoder().decode([String: String].self, from: hello.body)
		// HMAC-SHA256 keyed by the token over "vaultday-server-proof\n" + challenge, computed apart from ServerProof.
		let key = SymmetricKey(data: Data(Self.token.utf8))
		let expected = HMAC<SHA256>.authenticationCode(for: Data("vaultday-server-proof\n\(challenge)".utf8), using: key).map { String(format: "%02x", $0) }.joined()
		XCTAssertEqual(answer["proof"], expected)
		XCTAssertNotEqual(ServerProof.proof(challenge: challenge, token: String(repeating: "cd", count: 32)), expected)

		let missing = try await curl(port: port, path: "/v1/hello", token: nil)
		XCTAssertEqual(missing.status, 400)
		let short = try await curl(port: port, path: "/v1/hello", token: nil, extraHeaders: ["X-Vaultday-Challenge: abc"])
		XCTAssertEqual(short.status, 400)
		let posted = try await curl(port: port, path: "/v1/hello", method: "POST", token: nil, extraHeaders: ["X-Vaultday-Challenge: \(challenge)"])
		XCTAssertEqual(posted.status, 405)
		XCTAssertEqual(backend.calls, [], "hello never reaches the backend")
	}

	func testAMissingOrWrongTokenIsRefusedWith401() async throws {
		let backend = FakeTimerBackend()
		let (server, port) = try await startServer(backend: backend)
		defer { server.stop() }
		let missing = try await curl(port: port, path: "/v1/status", token: nil)
		XCTAssertEqual(missing.status, 401)
		let wrong = try await curl(port: port, path: "/v1/status", token: String(repeating: "cd", count: 32))
		XCTAssertEqual(wrong.status, 401)
		XCTAssertEqual(try errorMessage(wrong.body), "Unauthorized")
		let basic = try await curl(port: port, path: "/v1/status", token: nil, extraHeaders: ["Authorization: Basic \(Self.token)"])
		XCTAssertEqual(basic.status, 401)
		XCTAssertEqual(backend.calls, [], "nothing reached the backend")
	}

	func testABodyOver64KBIsRefusedWith413() async throws {
		let backend = FakeTimerBackend()
		let (server, port) = try await startServer(backend: backend)
		defer { server.stop() }
		let large = "{\"target\":\"" + String(repeating: "x", count: 70 * 1024) + "\"}"
		let refused = try await curl(port: port, path: "/v1/timer/start", method: "POST", token: Self.token, body: large)
		XCTAssertEqual(refused.status, 413)
		XCTAssertEqual(backend.calls, [])
		let parsed = HTTPRequestParser.parse(Data("POST /v1/timer/start HTTP/1.1\r\nContent-Length: 65537\r\n\r\n".utf8))
		guard case let .refused(response) = parsed else { return XCTFail("\(parsed)") }
		XCTAssertEqual(response.status, 413, "refused as soon as the length is known")
	}

	// MARK: Routes

	func testEveryRouteAnswersJSONFromTheBackend() async throws {
		let backend = FakeTimerBackend()
		let (server, port) = try await startServer(backend: backend)
		defer { server.stop() }

		let status = try await curl(port: port, path: "/v1/status", token: Self.token)
		XCTAssertEqual(status.status, 200)
		XCTAssertEqual(try JSONDecoder().decode(TimerAPIStatus.self, from: status.body), backend.idle)

		let today = try await curl(port: port, path: "/v1/today", token: Self.token)
		XCTAssertEqual(try JSONDecoder().decode(TimerAPIToday.self, from: today.body).tasks.map(\.taskId), ["task-5"])

		let started = try await curl(port: port, path: "/v1/timer/start", method: "POST", token: Self.token, body: #"{"target":"task-5","minutesAgo":10}"#)
		XCTAssertEqual(started.status, 201)
		XCTAssertEqual(try JSONDecoder().decode(TimerAPIStatus.self, from: started.body).timer?.taskId, "task-5")

		let again = try await curl(port: port, path: "/v1/timer/start", method: "POST", token: Self.token, body: #"{"target":"task-5"}"#)
		XCTAssertEqual(again.status, 409)
		XCTAssertEqual(try errorMessage(again.body), "The timer already runs")

		let stopped = try await curl(port: port, path: "/v1/timer/stop", method: "POST", token: Self.token)
		XCTAssertEqual(stopped.status, 200)
		XCTAssertEqual(try JSONDecoder().decode(TimerAPIStatus.self, from: stopped.body).logLine, "- 10:00 session · Fake · 10m [kind:: session] [id:: ses-1]")

		let continued = try await curl(port: port, path: "/v1/timer/continue", method: "POST", token: Self.token)
		XCTAssertEqual(continued.status, 201)

		let done = try await curl(port: port, path: "/v1/tasks/task-5/done", method: "POST", token: Self.token)
		XCTAssertEqual(done.status, 200)
		XCTAssertEqual(try JSONDecoder().decode(TimerAPIDone.self, from: done.body), TimerAPIDone(taskId: "task-5", isDone: true))

		let unknown = try await curl(port: port, path: "/v1/tasks/task-99/done", method: "POST", token: Self.token)
		XCTAssertEqual(unknown.status, 404)

		XCTAssertEqual(backend.calls, ["status", "today", "start task-5 10", "start task-5 nil", "stop", "continue", "done task-5", "done task-99"])
	}

	func testBadRequestsAreRefusedBeforeTheBackend() async throws {
		let backend = FakeTimerBackend()
		let (server, port) = try await startServer(backend: backend)
		defer { server.stop() }
		let noTarget = try await curl(port: port, path: "/v1/timer/start", method: "POST", token: Self.token, body: #"{"task":"x"}"#)
		XCTAssertEqual(noTarget.status, 400)
		let wrongMethod = try await curl(port: port, path: "/v1/timer/stop", method: "GET", token: Self.token)
		XCTAssertEqual(wrongMethod.status, 405)
		let noRoute = try await curl(port: port, path: "/v2/anything", token: Self.token)
		XCTAssertEqual(noRoute.status, 404)
		XCTAssertEqual(backend.calls, [])
	}

	// MARK: Helpers

	private func startServer(backend: FakeTimerBackend) async throws -> (LocalAPIServer, UInt16) {
		let router = TimerAPIRouter(token: Self.token, backend: backend)
		let server = LocalAPIServer(router: router)
		let port = try await server.start(port: 0)
		XCTAssertNotEqual(port, 0)
		return (server, port)
	}

	private func permissions(of url: URL) throws -> Int {
		let attributes = try FileManager.default.attributesOfItem(atPath: url.path)
		return try XCTUnwrap(attributes[.posixPermissions] as? NSNumber).intValue
	}

	private func errorMessage(_ body: Data) throws -> String? {
		try JSONDecoder().decode([String: String].self, from: body)["error"]
	}

	/// The first IPv4 address of an interface that is up and not loopback.
	static func nonLoopbackIPv4Address() -> String? {
		var interfaces: UnsafeMutablePointer<ifaddrs>?
		guard getifaddrs(&interfaces) == 0, let first = interfaces else { return nil }
		defer { freeifaddrs(interfaces) }
		for pointer in sequence(first: first, next: { $0.pointee.ifa_next }) {
			let interface = pointer.pointee
			let flags = Int32(interface.ifa_flags)
			guard let address = interface.ifa_addr, address.pointee.sa_family == UInt8(AF_INET),
			      flags & IFF_UP != 0, flags & IFF_LOOPBACK == 0
			else { continue }
			var host = [CChar](repeating: 0, count: Int(NI_MAXHOST))
			guard getnameinfo(address, socklen_t(address.pointee.sa_len), &host, socklen_t(host.count), nil, 0, NI_NUMERICHOST) == 0 else { continue }
			return String(cString: host)
		}
		return nil
	}
}

/// What curl got: the status (0 when it reached nothing) and the body.
struct CurlResult {
	let status: Int
	let body: Data
}

/// Runs `/usr/bin/curl` against the server, off the main actor.
func curl(port: UInt16, path: String, method: String = "GET", token: String?, body: String? = nil, host: String = "127.0.0.1", extraHeaders: [String] = []) async throws -> CurlResult {
	var arguments = ["-s", "--max-time", "3", "-o", "-", "-w", "\n%{http_code}", "-X", method]
	if let token { arguments += ["-H", "Authorization: Bearer \(token)"] }
	for header in extraHeaders { arguments += ["-H", header] }
	if let body { arguments += ["-H", "Content-Type: application/json", "--data-binary", "@-"] }
	arguments.append("http://\(host):\(port)\(path)")
	let finalArguments = arguments
	return try await Task.detached {
		let process = Process()
		process.executableURL = URL(fileURLWithPath: "/usr/bin/curl")
		process.arguments = finalArguments
		let output = Pipe()
		let input = Pipe()
		process.standardOutput = output
		process.standardInput = input
		try process.run()
		if let body { input.fileHandleForWriting.write(Data(body.utf8)) }
		try input.fileHandleForWriting.close()
		let data = output.fileHandleForReading.readDataToEndOfFile()
		process.waitUntilExit()
		let text = String(data: data, encoding: .utf8) ?? ""
		let lastNewline = text.lastIndex(of: "\n") ?? text.startIndex
		let code = Int(text[text.index(after: lastNewline)...]) ?? 0
		return CurlResult(status: code, body: Data(text[..<lastNewline].utf8))
	}.value
}

/// A backend that answers canned values and records what it was asked.
@MainActor
final class FakeTimerBackend: TimerAPIBackend {
	private(set) var calls: [String] = []
	private var isRunning = false
	let idle = TimerAPIStatus(workspace: "Fake", timer: nil, day: TimerAPIDay(date: "2026-10-23", spentSeconds: 0, loggedSeconds: 0, workloadSeconds: 28800))

	private var running: TimerAPIStatus {
		TimerAPIStatus(workspace: "Fake", timer: TimerAPITimer(sessionId: "ses-1", taskId: "task-5", story: nil, text: "Fake", isPaused: false, startedAt: "2026-10-23T10:00:00Z", elapsedSeconds: 600), day: idle.day)
	}

	func status() -> TimerAPIStatus {
		calls.append("status")
		return isRunning ? running : idle
	}

	func today() -> TimerAPIToday {
		calls.append("today")
		return TimerAPIToday(workspace: "Fake", date: "2026-10-23", tasks: [TimerAPITask(taskId: "task-5", text: "Fake", story: nil, isDone: false, estimateMinutes: 30)])
	}

	func startTimer(target: String, minutesAgo: Int?) -> Result<TimerAPIStatus, TimerAPIFailure> {
		calls.append("start \(target) \(minutesAgo.map(String.init) ?? "nil")")
		guard !isRunning else { return .failure(TimerAPIFailure(.conflict, "The timer already runs")) }
		isRunning = true
		return .success(running)
	}

	func stopTimer() -> Result<TimerAPIStatus, TimerAPIFailure> {
		calls.append("stop")
		isRunning = false
		return .success(TimerAPIStatus(workspace: "Fake", timer: nil, day: idle.day, logLine: "- 10:00 session · Fake · 10m [kind:: session] [id:: ses-1]"))
	}

	func continueTimer() -> Result<TimerAPIStatus, TimerAPIFailure> {
		calls.append("continue")
		isRunning = true
		return .success(running)
	}

	func completeTask(id: String) -> Result<TimerAPIDone, TimerAPIFailure> {
		calls.append("done \(id)")
		return id == "task-5" ? .success(TimerAPIDone(taskId: id, isDone: true)) : .failure(TimerAPIFailure(.notFound, "No task \(id)"))
	}
}
