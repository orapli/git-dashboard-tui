# v0.5.2 — Open Hunk without flashing the terminal

Opening a diff in the Hunk GUI now keeps the dashboard visible.

- `hunk` and `hunkdiff` launch in the background without taking over terminal input
  or output, removing the brief alternate-screen flash.
- Bare command names, absolute paths and Windows `.exe` names are recognised.
- Other external diff commands keep their existing blocking terminal handoff, so
  terminal-based tools continue to work as before.

The release includes the search improvements from v0.5.1 and remains compatible with
existing configuration and cache files.

Pre-built binaries cover Linux x86_64/ARM64, macOS Intel/Apple Silicon and Windows x64.
Verify downloaded archives with `SHA256SUMS`. On Linux/macOS, install the latest release with:

```bash
curl -fsSL https://raw.githubusercontent.com/orapli/git-dashboard-tui/main/install.sh | sh
```

Building from source requires Rust 1.88 or later.

# 日本語

HunkのGUIで差分を開くときも、ダッシュボードが表示されたままになりました。

- `hunk`と`hunkdiff`は端末の入出力を引き継がずバックグラウンドで起動し、
  代替画面が一瞬表示される現象をなくしました。
- 裸のコマンド名、絶対パス、Windowsの`.exe`名を認識します。
- それ以外の外部diffコマンドは従来どおり終了まで端末を引き渡すため、
  端末型ツールの動作は変わりません。

v0.5.1の検索改善を含み、既存の設定とキャッシュファイルはそのまま利用できます。

Linux x86_64/ARM64、macOS Intel/Apple Silicon、Windows x64向けのバイナリを提供します。
ダウンロードしたアーカイブは`SHA256SUMS`で検証できます。ソースからのビルドにはRust 1.88以上が必要です。
