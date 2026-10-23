import SwiftUI

/// The shared icon motion modifiers. Each follows ``IconMotionGate``: no
/// motion on the first render, one effect per change, a crossfade under
/// Reduce Motion. The draw effects are SF Symbols 7's (macOS 26); on macOS
/// 15 an icon appears, disappears or is replaced with the effects that
/// release has, and a variable value is shown without the draw mode.
///
/// Spellings checked against the macOS 26.5 SDK: `DrawOnSymbolEffect`
/// (`.drawOn`, `.byLayer`, `.wholeSymbol`, `.individually`) and
/// `DrawOffSymbolEffect` (`.drawOff`, `.reversed`), both
/// `TransitionSymbolEffect` and `IndefiniteSymbolEffect`, macOS 26;
/// `Transition.symbolEffect(_:options:)`;
/// `ContentTransition.symbolEffect(.replace)` and
/// `ReplaceSymbolEffect.magic(fallback:)`; `View.symbolVariableValueMode(.draw)`.
extension View {
	/// An icon that changes state (play and pause, lock and unlock): the
	/// symbol is replaced, shared strokes staying put, when `value` changes
	/// after the first render.
	public func iconReplace(value: some Equatable & Sendable) -> some View {
		modifier(IconReplaceModifier(value: value))
	}

	/// Progress drawn along the symbol's path (`variableValue`), animating
	/// between values after the first render; colour stays flat.
	public func iconVariableDraw(value: Double) -> some View {
		modifier(IconVariableDrawModifier(value: value, isSymbol: true))
	}

	/// The same rule for a progress drawn without a symbol (the logged and
	/// tracked rings, a progress bar): the stroke runs to a new value after
	/// the first render, and jumps there under Reduce Motion.
	public func progressDraw(value: Double) -> some View {
		modifier(IconVariableDrawModifier(value: value, isSymbol: false))
	}
}

/// Shows `content` while `isShown`, drawing it on when it appears and off
/// when it leaves, after the first render only: the tick of a task done,
/// the lock of a locked workspace, a signal's source mark.
public struct IconPresence<Content: View>: View {
	let isShown: Bool
	let content: Content
	@State private var hasRendered = false
	@Environment(\.accessibilityReduceMotion) private var reduceMotion

	public init(isShown: Bool, @ViewBuilder content: () -> Content) {
		self.isShown = isShown
		self.content = content()
	}

	public var body: some View {
		// Insertion and removal are the only changes a transition plays on,
		// so each change plays one effect.
		let plan = IconMotionGate<Bool>.plan(isShown ? .drawOn : .drawOff, hasRendered: hasRendered, isChange: true, reduceMotion: reduceMotion)
		ZStack {
			if isShown {
				content.transition(IconMotionStyle.presenceTransition(plan))
			}
		}
		.animation(IconMotionStyle.animation(plan), value: isShown)
		.onAppear { hasRendered = true }
	}
}

/// The SwiftUI values behind a plan.
enum IconMotionStyle {
	static func animation(_ plan: IconMotionPlan) -> Animation? {
		plan == .none ? nil : .easeOut(duration: IconMotionTiming.crossfade)
	}

	static func presenceTransition(_ plan: IconMotionPlan) -> AnyTransition {
		switch plan {
		case .none: .identity
		case .crossfade: .opacity
		case .effect:
			if #available(macOS 26.0, *) {
				.asymmetric(
					insertion: AnyTransition(.symbolEffect(.drawOn.byLayer)),
					removal: AnyTransition(.symbolEffect(.drawOff.byLayer)),
				)
			} else {
				.asymmetric(
					insertion: AnyTransition(.symbolEffect(.appear.byLayer)),
					removal: AnyTransition(.symbolEffect(.disappear.byLayer)),
				)
			}
		}
	}

	static func replaceTransition(_ plan: IconMotionPlan) -> ContentTransition {
		switch plan {
		case .none: .identity
		case .crossfade: .opacity
		case .effect: .symbolEffect(.replace.magic(fallback: .replace.byLayer))
		}
	}
}

private struct IconReplaceModifier<Value: Equatable & Sendable>: ViewModifier {
	let value: Value
	@State private var hasRendered = false
	@Environment(\.accessibilityReduceMotion) private var reduceMotion

	func body(content: Content) -> some View {
		let plan = IconMotionGate<Value>.plan(.replace, hasRendered: hasRendered, isChange: true, reduceMotion: reduceMotion)
		content
			.contentTransition(IconMotionStyle.replaceTransition(plan))
			.animation(IconMotionStyle.animation(plan), value: value)
			.onAppear { hasRendered = true }
	}
}

private struct IconVariableDrawModifier: ViewModifier {
	let value: Double
	let isSymbol: Bool
	@State private var hasRendered = false
	@Environment(\.accessibilityReduceMotion) private var reduceMotion

	func body(content: Content) -> some View {
		// Under Reduce Motion the value is set without a draw between values.
		let plan = IconMotionGate<Double>.plan(.variableDraw, hasRendered: hasRendered, isChange: true, reduceMotion: reduceMotion)
		drawMode(content)
			.animation(plan == .effect(.variableDraw) ? .easeOut(duration: IconMotionTiming.crossfade) : nil, value: value)
			.onAppear { hasRendered = true }
	}

	@ViewBuilder
	private func drawMode(_ content: Content) -> some View {
		if #available(macOS 26.0, *), isSymbol {
			content.symbolVariableValueMode(.draw)
		} else {
			content
		}
	}
}
