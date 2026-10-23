import Foundation
import Network

/// The app's localhost HTTP API: a small Network-framework server bound to
/// 127.0.0.1 only. Every connection's peer is checked to be loopback again
/// before a byte is read, and each request goes to the ``TimerAPIRouter`` on
/// the main actor, where the app's models live. One request per connection.
public final class LocalAPIServer: @unchecked Sendable {
	/// The port the app prefers, so a CLI on a stale port file still finds it.
	public static let preferredPort: UInt16 = 47390
	/// A connection that has not sent a whole request by then is dropped.
	static let requestTimeout: TimeInterval = 10

	private let router: TimerAPIRouter
	private let queue = DispatchQueue(label: "com.eiluviann.vaultday.local-api")
	// Touched only on `queue`.
	private var listener: NWListener?

	public init(router: TimerAPIRouter) {
		self.router = router
	}

	/// Listens on 127.0.0.1:`port` (0 picks a free one) and returns the port.
	public func start(port: UInt16) async throws -> UInt16 {
		let parameters = NWParameters.tcp
		parameters.requiredLocalEndpoint = .hostPort(host: .ipv4(.loopback), port: NWEndpoint.Port(rawValue: port) ?? .any)
		parameters.requiredInterfaceType = .loopback
		let listener: NWListener
		do {
			listener = try NWListener(using: parameters)
		} catch {
			throw LocalAPIError.listenerFailed(error.localizedDescription)
		}
		return try await withCheckedThrowingContinuation { continuation in
			let resumed = ResumeOnce(continuation)
			listener.stateUpdateHandler = { state in
				switch state {
				case .ready:
					resumed.resume(returning: listener.port?.rawValue ?? port)
				case let .failed(error):
					listener.cancel()
					resumed.resume(throwing: LocalAPIError.listenerFailed(error.localizedDescription))
				case let .waiting(error):
					listener.cancel()
					resumed.resume(throwing: LocalAPIError.listenerFailed(error.localizedDescription))
				default:
					break
				}
			}
			listener.newConnectionHandler = { [weak self] connection in
				self?.accept(connection)
			}
			queue.async { [weak self] in
				self?.listener = listener
				listener.start(queue: self?.queue ?? .main)
			}
		}
	}

	/// Starts on the preferred port, else on any free one.
	public func startOnPreferredPort() async throws -> UInt16 {
		do {
			return try await start(port: Self.preferredPort)
		} catch {
			return try await start(port: 0)
		}
	}

	public func stop() {
		queue.async { [weak self] in
			self?.listener?.cancel()
			self?.listener = nil
		}
	}

	/// Whether `endpoint` is this Mac: 127.0.0.0/8, ::1 or `localhost`.
	public static func isLoopback(_ endpoint: NWEndpoint) -> Bool {
		guard case let .hostPort(host, _) = endpoint else { return false }
		switch host {
		case let .ipv4(address): return isLoopback(address)
		case let .ipv6(address): return isLoopback(address)
		case let .name(name, _):
			if let address = IPv4Address(name) { return isLoopback(address) }
			if let address = IPv6Address(name) { return isLoopback(address) }
			return name == "localhost"
		@unknown default: return false
		}
	}

	/// 127.0.0.0/8; `IPv4Address.isLoopback` answers for 127.0.0.1 only.
	private static func isLoopback(_ address: IPv4Address) -> Bool {
		address.rawValue.first == 127
	}

	/// ::1, or an IPv4-mapped loopback address.
	private static func isLoopback(_ address: IPv6Address) -> Bool {
		address.isLoopback || address.asIPv4.map(isLoopback) == true
	}

	private func accept(_ connection: NWConnection) {
		let isLoopback = Self.isLoopback(connection.endpoint)
		connection.start(queue: queue)
		guard isLoopback else {
			// Refused before reading anything; the listener's bind makes this unreachable in practice.
			send(.error(403, "Only this Mac may call Vaultday"), on: connection)
			return
		}
		queue.asyncAfter(deadline: .now() + Self.requestTimeout) { [weak connection] in
			connection?.cancel()
		}
		receive(on: connection, buffer: Data())
	}

	private func receive(on connection: NWConnection, buffer: Data) {
		connection.receive(minimumIncompleteLength: 1, maximumLength: 64 * 1024) { [weak self] content, _, isComplete, error in
			guard let self else { return }
			var received = buffer
			if let content { received.append(content) }
			switch HTTPRequestParser.parse(received) {
			case .incomplete:
				if isComplete || error != nil {
					connection.cancel()
				} else {
					self.receive(on: connection, buffer: received)
				}
			case let .refused(response):
				self.send(response, on: connection)
			case let .complete(request):
				let router = self.router
				Task { @MainActor in
					let response = router.respond(to: request, peerIsLoopback: true)
					self.send(response, on: connection)
				}
			}
		}
	}

	private func send(_ response: HTTPResponse, on connection: NWConnection) {
		connection.send(content: response.serialized(), contentContext: .finalMessage, isComplete: true, completion: .contentProcessed { _ in
			connection.cancel()
		})
	}
}

/// Resumes a continuation at most once, whichever listener state comes first.
private final class ResumeOnce<Value: Sendable>: @unchecked Sendable {
	private let lock = NSLock()
	private var continuation: CheckedContinuation<Value, any Error>?

	init(_ continuation: CheckedContinuation<Value, any Error>) {
		self.continuation = continuation
	}

	func resume(returning value: Value) {
		take()?.resume(returning: value)
	}

	func resume(throwing error: any Error) {
		take()?.resume(throwing: error)
	}

	private func take() -> CheckedContinuation<Value, any Error>? {
		lock.withLock {
			defer { continuation = nil }
			return continuation
		}
	}
}
