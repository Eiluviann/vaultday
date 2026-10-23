//! The story screen's render model against the synthetic mock vault.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::path::Path;

use vault_base::ops::open_vault;
use vault_base::{
    AppView, AuthorshipKind, CalloutFold, EmbedKind, FileAccess, PlainFileAccess, RenderBlock,
    Span, SpanLink, authorship, render_note,
};

use common::{sandbox, workspace};

const STORY: &str = "Projects/Website/WEB-142 Checkout redesign/WEB-142 Checkout redesign.md";
const RESEARCH: &str = "Projects/Website/WEB-142 Checkout redesign/Research notes.md";

/// The note's blocks with links resolved, and the sandbox that holds the files.
fn rendered(relative: &str) -> (Vec<RenderBlock>, common::Sandbox) {
    let sandbox = sandbox();
    let vault = open_vault(&workspace(&sandbox.root), &PlainFileAccess).unwrap();
    let path = Path::new(relative);
    let mut blocks = render_note(&vault.note(path).unwrap().document);
    vault.resolve_links(&mut blocks, path, &|candidate| {
        PlainFileAccess.stat(candidate).ok().flatten().is_some()
    });
    (blocks, sandbox)
}

fn all_spans(blocks: &[RenderBlock]) -> Vec<Span> {
    let mut spans = Vec::new();
    for block in blocks {
        match block {
            RenderBlock::Heading { spans: own, .. } | RenderBlock::Paragraph { spans: own, .. } => {
                spans.extend(own.iter().cloned());
            }
            RenderBlock::Quote { children, .. } | RenderBlock::Callout { children, .. } => {
                spans.extend(all_spans(children));
            }
            RenderBlock::List { items, .. } => {
                for item in items {
                    spans.extend(item.spans.iter().cloned());
                }
            }
            _ => {}
        }
    }
    spans
}

#[test]
fn the_story_renders_without_frontmatter_title_or_markers() {
    let (blocks, _sandbox) = rendered(STORY);
    let text = format!("{blocks:?}");
    assert!(!text.contains("type: story"), "{text}");
    assert!(
        !text.contains("ai:begin fable · 2026-10-20"),
        "markers never show: {text}"
    );
    assert!(!text.contains("ai:end"), "{text}");
    assert!(
        !blocks
            .iter()
            .any(|block| matches!(block, RenderBlock::Heading { level: 1, .. })),
        "the title is the screen's header"
    );
    // The marker quoted inside the Background fence is code, shown as written.
    assert!(blocks.iter().any(|block| matches!(
        block,
        RenderBlock::Code { language: Some(language), text, .. }
            if language == "markdown" && text.contains("%% ai:begin fable · 2026-10-01")
    )));
}

#[test]
fn the_task_sections_and_the_activity_query_are_the_app_s_views() {
    let (blocks, _sandbox) = rendered(STORY);
    let views: Vec<AppView> = blocks
        .iter()
        .filter_map(|block| match block {
            RenderBlock::AppView { view, .. } => Some(*view),
            RenderBlock::Query { view, .. } => *view,
            _ => None,
        })
        .collect();
    assert_eq!(
        views,
        [AppView::NextSteps, AppView::PendingInput, AppView::Activity]
    );
    let text = format!("{blocks:?}");
    assert!(
        !text.contains("Wire the payment form"),
        "the accordion draws the steps"
    );
}

#[test]
fn the_body_checkbox_carries_the_id_the_writer_needs() {
    let (blocks, _sandbox) = rendered(STORY);
    let checkbox = blocks
        .iter()
        .find_map(|block| match block {
            RenderBlock::List { items, .. } => items.first()?.checkbox.clone(),
            _ => None,
        })
        .unwrap();
    assert_eq!(checkbox.task_id.as_deref(), Some("task-1"));
}

#[test]
fn wikilinks_resolve_to_vault_notes_and_the_image_to_its_file() {
    let (blocks, sandbox) = rendered(STORY);
    let links: Vec<(String, SpanLink)> = all_spans(&blocks)
        .into_iter()
        .filter_map(|span| span.link.map(|link| (span.text, link)))
        .collect();
    let research = links
        .iter()
        .find(|(text, _)| text == "the competitor sweep")
        .unwrap();
    let SpanLink::Note(target) = &research.1 else {
        panic!("{research:?}");
    };
    assert_eq!(target.resolved.as_deref(), Some(Path::new(RESEARCH)));
    assert_eq!(target.section.as_deref(), Some("Checkout research"));
    let image = blocks
        .iter()
        .find_map(|block| match block {
            RenderBlock::Embed {
                kind: EmbedKind::Image,
                target,
                ..
            } => Some(target.clone()),
            _ => None,
        })
        .unwrap();
    let resolved = image.resolved.unwrap();
    assert!(sandbox.root.join(&resolved).is_file(), "{resolved:?}");
    assert!(blocks.iter().any(
        |block| matches!(block, RenderBlock::Callout { kind, fold: CalloutFold::Static, .. } if kind == "note")
    ));
}

#[test]
fn the_research_note_holds_a_table_a_plugin_embed_and_a_plain_query() {
    let (blocks, _sandbox) = rendered(RESEARCH);
    assert!(
        blocks
            .iter()
            .any(|block| matches!(block, RenderBlock::Table { rows, .. } if rows.len() == 2))
    );
    assert!(blocks.iter().any(|block| matches!(
        block,
        RenderBlock::Embed { kind: EmbedKind::Plugin(name), target, .. }
            if name == "Excalidraw" && target.resolved.is_none()
    )));
    assert!(blocks.iter().any(
        |block| matches!(block, RenderBlock::Query { view: None, language, .. } if language == "dataview")
    ));
    let callout_link = blocks
        .iter()
        .find_map(|block| match block {
            RenderBlock::Callout {
                fold: CalloutFold::Folded,
                children,
                ..
            } => all_spans(children).into_iter().find_map(|span| span.link),
            _ => None,
        })
        .unwrap();
    assert!(matches!(
        callout_link,
        SpanLink::Note(target) if target.resolved.as_deref() == Some(Path::new(STORY))
    ));
    assert!(!format!("{blocks:?}").contains("ai:note"));
}

#[test]
fn authorship_and_project_come_from_the_vault() {
    let sandbox = sandbox();
    let vault = open_vault(&workspace(&sandbox.root), &PlainFileAccess).unwrap();
    let story = vault.note(Path::new(STORY)).unwrap();
    let summary = authorship(&story.document);
    assert_eq!(summary.kind(), AuthorshipKind::Mixed);
    assert_eq!(summary.model_units, 1);
    let research = vault.note(Path::new(RESEARCH)).unwrap();
    assert_eq!(authorship(&research.document).kind(), AuthorshipKind::Model);
    assert_eq!(
        vault.project_of(Path::new(STORY)).as_deref(),
        Some("Website")
    );
    assert_eq!(vault.project_of(Path::new("Home.md")), None);
}

#[test]
fn embeds_and_links_never_resolve_outside_the_vault() {
    let sandbox = sandbox();
    let vault = open_vault(&workspace(&sandbox.root), &PlainFileAccess).unwrap();
    let document = vault_base::Document::parse(
        "# Escape\n![[../../../../outside.png]]\n![[/etc/outside.png]]\n![[sub/../../../../../outside.png]]\nSee [[../../../../outside.pdf]].\n",
    );
    let mut blocks = render_note(&document);
    // Every candidate "exists", so only the vault-root rule can refuse it.
    vault.resolve_links(&mut blocks, Path::new(STORY), &|_| true);
    let text = format!("{blocks:?}");
    assert!(!text.contains("resolved: Some"), "{text}");
}

#[test]
fn markers_inside_quotes_and_lists_are_hidden() {
    let document = vault_base::Document::parse(
        "# Markers\n> Quoted\n> %% ai:note fable · 2026-10-21 %%\n> %% ai:begin fable · 2026-10-21 · q1 %%\n> Inside\n> %% ai:end %%\n\n- Item\n  %% ai:note fable · 2026-10-21 %%\n- Next %% ai:note fable · 2026-10-21 %%\n",
    );
    let text = format!("{:?}", render_note(&document));
    assert!(!text.contains("ai:"), "{text}");
    assert!(text.contains("Inside") && text.contains("Next"), "{text}");
}
