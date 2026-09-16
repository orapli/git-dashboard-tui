# v0.5.4 — Read every line of a commit

The Commits preview now keeps the complete commit message and changed-file list available.

- Use `PageUp` / `PageDown` to scroll the selected commit's details.
- Point at the Commit pane and use the mouse wheel for the same operation without moving
  the selected commit.
- The pane title shows the current line and available controls.
- Selecting another commit returns its preview to the first line.

The release includes the interactive Hunk handoff fix from v0.5.3 and remains compatible
with existing configuration and cache files.

Pre-built binaries cover Linux x86_64/ARM64, macOS Intel/Apple Silicon and Windows x64.
Verify downloaded archives with `SHA256SUMS`. On Linux/macOS, install the latest release with:

```bash
curl -fsSL https://raw.githubusercontent.com/orapli/git-dashboard-tui/main/install.sh | sh
```

Building from source requires Rust 1.88 or later.

# 日本語

Commitsの詳細ペインで、コミットメッセージ全文と変更ファイル一覧を確認できるようになりました。

- `PageUp` / `PageDown` で選択コミットの詳細をスクロールできます。
- コミット内容ペイン上では、選択コミットを動かさずマウスホイールでもスクロールできます。
- ペインのタイトルに現在行と操作方法を表示します。
- 別のコミットを選択すると、その詳細の先頭へ戻ります。

v0.5.3のHunk連携修正を含み、既存の設定とキャッシュファイルはそのまま利用できます。

Linux x86_64/ARM64、macOS Intel/Apple Silicon、Windows x64向けのバイナリを提供します。
ダウンロードしたアーカイブは`SHA256SUMS`で検証できます。ソースからのビルドにはRust 1.88以上が必要です。
