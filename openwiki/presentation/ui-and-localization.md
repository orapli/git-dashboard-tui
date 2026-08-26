---
type: presentation
title: Terminal rendering, themes, localization, and syntax tokens
description: Ratatui rendering boundary, semantic palettes, translated copy, and line-oriented syntax tokenization.
tags: [ui, ratatui, localization, syntax]
---

# Terminal rendering, themes, localization, and syntax tokens

`ui::draw` is the renderer entrypoint used by `src/lib.rs`. It resolves `Palette::for_name(app.theme_name())`, draws title/body/footer, dispatches one renderer per `Screen`, then overlays prompt and confirmation. UI rendering reads `App`; keyboard/mouse mutation stays in [application navigation](../application/navigation.md).

## Rendering contract

The main vertical layout is one title row, flexible content, and a two-row footer. `draw_home`, `draw_repo`, `draw_diff`, `draw_settings`, `draw_global_members`, `draw_repo_finder`, `draw_commit_search`, `draw_help`, and `draw_log` each render one inspected screen. The footer gives `App::error` precedence over status and active filter text. Prompts overlay the full frame and app interaction blocks normal mouse clicks while input or confirmation is active.

The renderer shows an elapsed-time, one-column spinner whenever [background work](../application/background-work.md) reports activity: a title-bar job count across screens, a `pull`/`fetch`/`refresh` label in the affected Home Sync cell, and loading-screen/footer prefixes. It does not own operation state. `draw_home` also records table-header bounds, while repository lists record `ListViewport` after Ratatui has selected an offset; [navigation](../application/navigation.md) consumes those values for click handling rather than replicating layout calculations.

Home puts interrupted-operation and conflict badges before PR/CI labels because fixed-width truncation must preserve the observation-only state the app cannot resolve. Commit graph ANSI styling is interpreted only for graph lanes. Git’s command boundary strips terminal control sequences from all other repository text, while commit-graph SGR is selectively retained for this renderer; content-bearing diff rows, previews, and oneline logs expand tabs to four-column stops before display. Diff rendering uses flattened `DiffLine` text and kind; while `FileDiff` carries tokens, current flattening does not forward token styles into this UI path.

## Themes and languages

`Palette` is a semantic role set (`bg`, surface, text, accent, green/yellow/red, border, six graph lane colors), not raw colors scattered throughout widgets. `Catppuccin Mocha` is default; `Catppuccin Latte` is supported; unknown persisted theme names fall back to Mocha. Tests assert both known theme resolution and fallback.

The crate-level `rust_i18n::i18n!("locales", fallback = "en")` loads `locales/en.yml` and `locales/ja.yml`. `i18n::t(Language, key)` maps `Language::English`/`Japanese` to catalog locale; `App::tt` handles inline English/Japanese copy. Missing keys fall back to their key according to rust-i18n. A new visible string should use the existing approach consistently and update both locale catalogs when it is a catalog key.

## Syntax tokens

`syntax::tokenize(line, ext)` dispatches by lowercased extension for Rust, Python, Ruby, JS/TS/JSON, TOML, YAML, HTML/XML, CSS, Markdown, and plain text. `Token` preserves original text and labels it Text, Keyword, Type, String, Comment, Number, or Punctuation. Tokenizers are line-oriented: do not assume multi-line lexical state. The Git diff pipeline tokenizes only after truncation; see [history and diffs](../git/history-and-diffs.md).

## Validation

Use `cargo test --lib colors::tests` for palette fallback and `cargo test --lib` for syntax/UI-adjacent unit tests. `src/ui.rs:activity_tests`, `sort_tests`, and `commit_click_tests` provide rendered-buffer checks for spinners and click geometry. Manual terminal validation remains necessary for geometry, narrow widths, mouse hit targets, Unicode width, theme contrast, prompt overlays, and hostile repository text; run `cargo run` in a disposable Git repository after `cargo fmt --check`.
