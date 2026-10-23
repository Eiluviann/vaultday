import Foundation

/// One HTTP/1.1 request as the localhost API reads it. Header names are lowercased.
public struct HTTPRequest: Sendable, Equatable {
	public let method: String
	public let path: String
	public let headers: [String: String]
	public let body: Data

	public init(method: String, path: String, headers: [String: String] = [:], body: Data = Data()) {
		self.method = method
		self.path = path
		self.headers = Dictionary(uniqueKeysWithValues: headers.map { ($0.key.lowercased(), $0.value) })
		self.body = body
	}
}

/// One response; always JSON, always `Connection: close`.
public struct HTTPResponse: Sendable, Equatable {
	public let status: Int
	public let body: Data

	public init(status: Int, body: Data) {
		self.status = status
		self.body = body
	}

	/// An `{"error": message}` body.
	public static func error(_ status: Int, _ message: String) -> HTTPResponse {
		let body = (try? JSONEncoder().encode(["error": message])) ?? Data()
		return HTTPResponse(status: status, body: body)
	}

	/// The bytes on the wire.
	public func serialized() -> Data {
		let head = [
			"HTTP/1.1 \(status) \(Self.reason(for: status))",
			"Content-Type: application/json; charset=utf-8",
			"Content-Length: \(body.count)",
			"Cache-Control: no-store",
			"Connection: close",
			"",
			"",
		].joined(separator: "\r\n")
		return Data(head.utf8) + body
	}

	static func reason(for status: Int) -> String {
		switch status {
		case 200: "OK"
		case 201: "Created"
		case 400: "Bad Request"
		case 401: "Unauthorized"
		case 403: "Forbidden"
		case 404: "Not Found"
		case 405: "Method Not Allowed"
		case 409: "Conflict"
		case 411: "Length Required"
		case 413: "Content Too Large"
		case 431: "Request Header Fields Too Large"
		case 503: "Service Unavailable"
		default: "Error"
		}
	}
}

/// What the bytes received so far make.
public enum HTTPParseOutcome: Sendable, Equatable {
	/// More bytes are needed.
	case incomplete
	case complete(HTTPRequest)
	/// The request is refused before it reaches a route.
	case refused(HTTPResponse)
}

/// A strict, small HTTP/1.1 request reader: a request line, headers, and a
/// body only by `Content-Length` (no chunked bodies). Bodies over 64 KB are
/// refused with 413 as soon as the length is known.
public enum HTTPRequestParser {
	public static let maximumBodyBytes = 64 * 1024
	public static let maximumHeaderBytes = 16 * 1024
	private static let headerEnd = Data("\r\n\r\n".utf8)

	public static func parse(_ data: Data) -> HTTPParseOutcome {
		guard let headerRange = data.range(of: headerEnd) else {
			return data.count > maximumHeaderBytes ? .refused(.error(431, "Headers too large")) : .incomplete
		}
		guard headerRange.lowerBound <= maximumHeaderBytes,
		      let head = String(data: data[data.startIndex ..< headerRange.lowerBound], encoding: .utf8)
		else { return .refused(.error(431, "Headers too large")) }
		var lines = head.components(separatedBy: "\r\n")
		let requestLine = lines.removeFirst().split(separator: " ", omittingEmptySubsequences: false)
		guard requestLine.count == 3, requestLine[2].hasPrefix("HTTP/1.") else {
			return .refused(.error(400, "Not an HTTP/1.1 request"))
		}
		var headers: [String: String] = [:]
		for line in lines {
			guard let colon = line.firstIndex(of: ":") else { return .refused(.error(400, "Malformed header")) }
			let name = line[..<colon].trimmingCharacters(in: .whitespaces).lowercased()
			headers[name] = line[line.index(after: colon)...].trimmingCharacters(in: .whitespaces)
		}
		if headers["transfer-encoding"] != nil {
			return .refused(.error(411, "Send a Content-Length body"))
		}
		let length: Int
		if let text = headers["content-length"] {
			guard let parsed = Int(text), parsed >= 0 else { return .refused(.error(400, "Bad Content-Length")) }
			length = parsed
		} else {
			length = 0
		}
		guard length <= maximumBodyBytes else { return .refused(.error(413, "Bodies are limited to 64 KB")) }
		let bodyStart = headerRange.upperBound
		guard data.count - (bodyStart - data.startIndex) >= length else { return .incomplete }
		let body = Data(data[bodyStart ..< bodyStart + length])
		let target = String(requestLine[1])
		let path = target.split(separator: "?", maxSplits: 1).first.map(String.init) ?? target
		return .complete(HTTPRequest(method: String(requestLine[0]), path: path, headers: headers, body: body))
	}
}
