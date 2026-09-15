# v0.5.1 — Search the list without losing your place

In-list search now tells you what it found and keeps the surrounding history in view.

- Commits and Branch Log keep a labelled search row and match count visible after you
  finish typing.
- Matching text is highlighted, while the rows around each result stay in the list so
  you can understand the change in context.
- Press `n` / `N` to move to the next or previous match, with wraparound. `Enter` moves
  to the first match, and `Esc` clears the search before leaving the screen.
- Repository tabs, Global Members and Workspace show the active filter, visible/total
  count and a clear no-matches message. Selection is preserved when the filter is cleared.

The release includes the existing v0.5.0 features and remains compatible with their
configuration and cache files.

Pre-built binaries cover Linux x86_64/ARM64, macOS Intel/Apple Silicon and Windows x64.
Verify downloaded archives with `SHA256SUMS`. On Linux/macOS, install the latest release with:

```bash
curl -fsSL https://raw.githubusercontent.com/orapli/git-dashboard-tui/main/install.sh | sh
```

Building from source requires Rust 1.88 or later.

# 日本語

一覧内検索で、検索内容と結果が分かりやすくなり、前後の履歴を見ながら移動できます。

- CommitsとBranch Logに検索欄と一致件数を表示し、入力を確定した後も残します。
- 一致した文字を強調し、結果の前後の行も一覧に残すため、変更の文脈を確認できます。
- `n` / `N` で次・前の一致へ循環移動できます。`Enter` は最初の一致へ移動し、
  `Esc` は検索を解除してから画面を離れます。
- Repo各タブ、Global Members、Workspaceでは、検索内容と表示件数/全件数を表示し、
  一致しない場合は案内します。絞り込みを解除しても選択位置を保ちます。

既存のv0.5.0の機能を含み、設定とキャッシュファイルはそのまま利用できます。

Linux x86_64/ARM64、macOS Intel/Apple Silicon、Windows x64向けのバイナリを提供します。
ダウンロードしたアーカイブは`SHA256SUMS`で検証できます。ソースからのビルドにはRust 1.88以上が必要です。
