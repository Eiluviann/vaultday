import CoreGraphics

/// Proportions of the setup window (mockup "Onboarding" boards, 1100 × 800):
/// a 38 px title bar over a 240 px step rail and the content pane.
enum SetupLayout {
	static let titleBarHeight: CGFloat = 38
	/// The step rail: the sidebar token.
	static let sidebarWidth: CGFloat = Tokens.Size.sizeSidebar
	/// The content pane's gutter, left, right and top.
	static let gutter: CGFloat = Tokens.Spacing.space32
	/// The widest a lede or paragraph runs.
	static let measure: CGFloat = Tokens.Size.sizeMeasure
	/// The narrowest content pane the steps lay out in.
	static let contentMinWidth: CGFloat = 560
	/// The Configure sheet (board 2a).
	static let configureSheetSize = CGSize(width: 860, height: 680)
	/// The Add-folder popover.
	static let popoverWidth: CGFloat = 340
	static let windowDefaultSize = CGSize(width: 1100, height: 800)
}
