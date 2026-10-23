import Foundation

/// Runs the localhost API for the app: makes or reads the token, starts the
/// loopback server on the preferred port (else a free one) and writes the
/// port file the CLI reads; removes it again at the stop.
@MainActor
@Observable
public final class LocalAPIService {
	/// What the API is doing, for Settings: where it listens, or why it does not.
	public private(set) var statusText = "The localhost API has not started"
	public private(set) var port: UInt16?

	private let environment: CoreEnvironment
	private let backend: any TimerAPIBackend
	@ObservationIgnored private var server: LocalAPIServer?

	public init(environment: CoreEnvironment, backend: any TimerAPIBackend) {
		self.environment = environment
		self.backend = backend
	}

	public func start() async {
		guard server == nil else { return }
		do {
			let token = try LocalAPICredentials.prepareToken(secrets: environment.secrets, layout: environment.layout)
			let server = LocalAPIServer(router: TimerAPIRouter(token: token, backend: backend))
			let port = try await server.startOnPreferredPort()
			try LocalAPICredentials.writePort(port, layout: environment.layout)
			self.server = server
			self.port = port
			statusText = "The command line reaches Vaultday on 127.0.0.1:\(port)"
		} catch {
			statusText = error.localizedDescription
		}
	}

	public func stop() {
		server?.stop()
		server = nil
		port = nil
		LocalAPICredentials.removePort(layout: environment.layout)
	}
}
