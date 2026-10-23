import Foundation

/// The running timer, as `GET /v1/status` answers it.
public struct TimerAPITimer: Codable, Sendable, Equatable {
	public let sessionId: String
	public let taskId: String?
	public let story: String?
	public let text: String
	public let isPaused: Bool
	/// RFC 3339.
	public let startedAt: String
	public let elapsedSeconds: Int

	public init(sessionId: String, taskId: String?, story: String?, text: String, isPaused: Bool, startedAt: String, elapsedSeconds: Int) {
		self.sessionId = sessionId
		self.taskId = taskId
		self.story = story
		self.text = text
		self.isPaused = isPaused
		self.startedAt = startedAt
		self.elapsedSeconds = elapsedSeconds
	}
}

/// Today's timer time: tracked, logged to a story, and the workload it is measured against.
public struct TimerAPIDay: Codable, Sendable, Equatable {
	public let date: String
	public let spentSeconds: Int
	public let loggedSeconds: Int
	public let workloadSeconds: Int

	public init(date: String, spentSeconds: Int, loggedSeconds: Int, workloadSeconds: Int) {
		self.date = date
		self.spentSeconds = spentSeconds
		self.loggedSeconds = loggedSeconds
		self.workloadSeconds = workloadSeconds
	}
}

/// The answer of `GET /v1/status` and of every timer route.
public struct TimerAPIStatus: Codable, Sendable, Equatable {
	public let workspace: String?
	public let timer: TimerAPITimer?
	public let day: TimerAPIDay
	/// The day-log line a stop wrote; only on `POST /v1/timer/stop`.
	public let logLine: String?

	public init(workspace: String?, timer: TimerAPITimer?, day: TimerAPIDay, logLine: String? = nil) {
		self.workspace = workspace
		self.timer = timer
		self.day = day
		self.logLine = logLine
	}
}

/// One task of `GET /v1/today`.
public struct TimerAPITask: Codable, Sendable, Equatable {
	public let taskId: String
	public let text: String
	public let story: String?
	public let isDone: Bool
	public let estimateMinutes: Int?

	public init(taskId: String, text: String, story: String?, isDone: Bool, estimateMinutes: Int?) {
		self.taskId = taskId
		self.text = text
		self.story = story
		self.isDone = isDone
		self.estimateMinutes = estimateMinutes
	}
}

/// The answer of `GET /v1/today`: the day's planned tasks in list order.
public struct TimerAPIToday: Codable, Sendable, Equatable {
	public let workspace: String?
	public let date: String
	public let tasks: [TimerAPITask]

	public init(workspace: String?, date: String, tasks: [TimerAPITask]) {
		self.workspace = workspace
		self.date = date
		self.tasks = tasks
	}
}

/// The answer of `POST /v1/tasks/<id>/done`.
public struct TimerAPIDone: Codable, Sendable, Equatable {
	public let taskId: String
	public let isDone: Bool

	public init(taskId: String, isDone: Bool) {
		self.taskId = taskId
		self.isDone = isDone
	}
}

/// A refusal the backend gives for a valid request.
public struct TimerAPIFailure: Error, Sendable, Equatable {
	public enum Kind: Sendable, Equatable {
		/// A session already runs, or none does.
		case conflict
		/// Nothing matches the task id or text.
		case notFound
		/// The input breaks a rule.
		case invalid
		/// No workspace is open.
		case unavailable
	}

	public let kind: Kind
	public let message: String

	public init(_ kind: Kind, _ message: String) {
		self.kind = kind
		self.message = message
	}

	var status: Int {
		switch kind {
		case .conflict: 409
		case .notFound: 404
		case .invalid: 400
		case .unavailable: 503
		}
	}
}

/// What the localhost API acts on: the app's models in the app, a fake in tests.
@MainActor
public protocol TimerAPIBackend: AnyObject {
	func status() -> TimerAPIStatus
	func today() -> TimerAPIToday
	/// Starts the timer on `target`: a task id, a story number, or text of an open task of today.
	func startTimer(target: String, minutesAgo: Int?) -> Result<TimerAPIStatus, TimerAPIFailure>
	func stopTimer() -> Result<TimerAPIStatus, TimerAPIFailure>
	func continueTimer() -> Result<TimerAPIStatus, TimerAPIFailure>
	func completeTask(id: String) -> Result<TimerAPIDone, TimerAPIFailure>
}

/// The body of `POST /v1/timer/start`.
struct TimerStartBody: Decodable {
	let target: String
	let minutesAgo: Int?
}

/// Routes one request: checks the peer and the bearer token first, then
/// the method and path. Routes:
///
/// - `GET /v1/hello` with `X-Vaultday-Challenge`, the one route without the
///   token: ``ServerProof`` answers `{"proof": "<hex>"}`
/// - `GET /v1/status`, `GET /v1/today`
/// - `POST /v1/timer/start` `{"target": "task-5", "minutesAgo": 10}`,
///   `POST /v1/timer/stop`, `POST /v1/timer/continue`
/// - `POST /v1/tasks/<id>/done`
@MainActor
public final class TimerAPIRouter {
	private let token: String
	private weak var backend: (any TimerAPIBackend)?
	private let encoder: JSONEncoder = {
		let encoder = JSONEncoder()
		encoder.outputFormatting = [.sortedKeys]
		return encoder
	}()

	public init(token: String, backend: any TimerAPIBackend) {
		self.token = token
		self.backend = backend
	}

	/// The response to `request` from a peer at `peerIsLoopback`.
	public func respond(to request: HTTPRequest, peerIsLoopback: Bool) -> HTTPResponse {
		guard peerIsLoopback else { return .error(403, "Only this Mac may call Vaultday") }
		if request.path == ServerProof.route { return hello(request) }
		guard let presented = TokenComparison.bearerToken(request.headers["authorization"]),
		      TokenComparison.isEqual(presented, token)
		else { return .error(401, "Unauthorized") }
		guard let backend else { return .error(503, "Vaultday is starting") }
		return route(request, backend: backend)
	}

	/// Proves this server holds the token without the caller sending it.
	private func hello(_ request: HTTPRequest) -> HTTPResponse {
		guard request.method == "GET" else { return .error(405, "Method not allowed") }
		guard let challenge = request.headers[ServerProof.challengeHeader], ServerProof.isWellFormedChallenge(challenge) else {
			return .error(400, "Send X-Vaultday-Challenge with 32 to 128 hex digits")
		}
		return json(200, ["proof": ServerProof.proof(challenge: challenge, token: token)])
	}

	private static let knownPaths: Set<String> = ["/v1/status", "/v1/today", "/v1/timer/start", "/v1/timer/stop", "/v1/timer/continue"]

	private func route(_ request: HTTPRequest, backend: any TimerAPIBackend) -> HTTPResponse {
		switch (request.method, request.path) {
		case ("GET", "/v1/status"):
			return json(200, backend.status())
		case ("GET", "/v1/today"):
			return json(200, backend.today())
		case ("POST", "/v1/timer/start"):
			guard let body = try? JSONDecoder().decode(TimerStartBody.self, from: request.body),
			      !body.target.trimmingCharacters(in: .whitespaces).isEmpty
			else { return .error(400, "Send {\"target\": \"<task id or text>\"}") }
			return answer(backend.startTimer(target: body.target, minutesAgo: body.minutesAgo), status: 201)
		case ("POST", "/v1/timer/stop"):
			return answer(backend.stopTimer(), status: 200)
		case ("POST", "/v1/timer/continue"):
			return answer(backend.continueTimer(), status: 201)
		default:
			if let taskId = Self.doneTaskId(request.path) {
				return request.method == "POST" ? answer(backend.completeTask(id: taskId), status: 200) : .error(405, "Method not allowed")
			}
			return Self.knownPaths.contains(request.path) ? .error(405, "Method not allowed") : .error(404, "No such route")
		}
	}

	/// The id in `/v1/tasks/<id>/done`.
	private static func doneTaskId(_ path: String) -> String? {
		let segments = path.split(separator: "/", omittingEmptySubsequences: false).map(String.init)
		guard segments.count == 5, segments[0].isEmpty, segments[1] == "v1", segments[2] == "tasks", segments[4] == "done", !segments[3].isEmpty else { return nil }
		return segments[3].removingPercentEncoding ?? segments[3]
	}

	private func answer(_ result: Result<some Encodable, TimerAPIFailure>, status: Int) -> HTTPResponse {
		switch result {
		case let .success(value): json(status, value)
		case let .failure(failure): .error(failure.status, failure.message)
		}
	}

	private func json(_ status: Int, _ value: some Encodable) -> HTTPResponse {
		do {
			return HTTPResponse(status: status, body: try encoder.encode(value))
		} catch {
			return .error(500, "Could not encode the answer")
		}
	}
}
