import AppKit
import Darwin
import Foundation

/// Measures cold start: process start (from the kernel's record of this pid)
/// to the first SwiftUI body, and to the first run-loop turn after it, which
/// is the earliest point the window's first frame can be on screen.
@MainActor
enum LaunchTimer {
	private static var hasMarked = false

	static func markFirstBody(todayMilliseconds: Double?) {
		guard !hasMarked else { return }
		hasMarked = true
		let bodyMs = millisecondsSinceProcessStart()
		DispatchQueue.main.async {
			let frameMs = millisecondsSinceProcessStart()
			let todayText = todayMilliseconds.map { String(format: "%.2f", $0) } ?? "n/a"
			let line = String(
				format: "vaultday-spike: first body %.1f ms, first frame %.1f ms after process start; today() %@ ms\n",
				bodyMs, frameMs, todayText,
			)
			FileHandle.standardError.write(Data(line.utf8))
			if LaunchConfiguration.shouldQuitAfterPaint {
				NSApplication.shared.terminate(nil)
			}
		}
	}

	private static func millisecondsSinceProcessStart() -> Double {
		guard let start = processStartTime() else { return .nan }
		return Date().timeIntervalSince(start) * 1000
	}

	private static func processStartTime() -> Date? {
		var info = kinfo_proc()
		var size = MemoryLayout<kinfo_proc>.stride
		var mib: [Int32] = [CTL_KERN, KERN_PROC, KERN_PROC_PID, getpid()]
		guard sysctl(&mib, UInt32(mib.count), &info, &size, nil, 0) == 0 else { return nil }
		let start = info.kp_proc.p_starttime
		return Date(timeIntervalSince1970: Double(start.tv_sec) + Double(start.tv_usec) / 1_000_000)
	}
}
