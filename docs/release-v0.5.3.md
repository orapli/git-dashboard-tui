# v0.5.3 — Restore interactive Hunk diffs

Hunk opens and accepts input again from the dashboard.

- Hunk receives terminal input and output and runs until you exit it.
- The dashboard suppresses its extra pre-launch command banner for Hunk, reducing the
  visible transition before Hunk draws.
- Other external diff commands retain their existing interactive handoff and banner.

This fixes the v0.5.2 regression that incorrectly treated the Hunk terminal UI as a
detached GUI application.

Pre-built binaries cover Linux x86_64/ARM64, macOS Intel/Apple Silicon and Windows x64.
Verify downloaded archives with `SHA256SUMS`. On Linux/macOS, install the latest release with:

```bash
curl -fsSL https://raw.githubusercontent.com/orapli/git-dashboard-tui/main/install.sh | sh
```

Building from source requires Rust 1.88 or later.

# 日本語

ダッシュボードからHunkを開き、再び操作できるようになりました。

- Hunkへ端末の入力・出力を引き渡し、終了するまで待ちます。
- Hunk起動前の余計なコマンド案内を抑止し、Hunkが描画されるまでの表示切り替えを減らします。
- それ以外の外部diffコマンドは、従来どおり対話的に端末を引き渡して案内を表示します。

Hunkの端末UIを切り離されたGUIアプリとして扱っていたv0.5.2の回帰を修正します。

Linux x86_64/ARM64、macOS Intel/Apple Silicon、Windows x64向けのバイナリを提供します。
ダウンロードしたアーカイブは`SHA256SUMS`で検証できます。ソースからのビルドにはRust 1.88以上が必要です。
