# v0.4.1 — Mouse navigation

Move between screens and select repositories with the mouse.

- Use Back, Home and Navigate in the title bar. Navigate opens Settings, Worktrees, Global Members or Commit Search.
- Select repositories and members in Settings; tab clicks follow the rendered English/Japanese layout.
- Select Repository Finder rows and toggle their checkboxes. Already registered repositories stay protected.
- Select members in Global Members, then select and click their repository row again to open it. Click a selected Commit Search result to open its commit.
- Keep clicks aligned after scrolling or filtering, and prevent background mouse operations while a dialog or navigation menu is open.

Pre-built binaries cover Linux x86_64/ARM64, macOS Intel/Apple Silicon and Windows x64.
Verify downloaded archives with `SHA256SUMS`. On Linux/macOS, install the latest release with:

```bash
curl -fsSL https://raw.githubusercontent.com/orapli/git-dashboard-tui/main/install.sh | sh
```

Building from source requires Rust 1.88 or later. Existing configuration remains compatible.

# 日本語

画面移動とリポジトリ選択のマウス操作を改善しました。

- タイトルバーの「戻る」「Home」「移動」から操作できます。「移動」は設定・Worktree・横断メンバー・コミット検索を開きます。
- 設定画面のリポジトリ・メンバーをクリック選択でき、日英表示でタブのクリック位置が一致します。
- Repository Finderで行選択とチェック切り替えができます。登録済みの項目は変更しません。
- 横断メンバーを選択し、右側の選択済みリポジトリ行を再クリックして開けます。選択済みのコミット検索結果をクリックすると、そのコミットを開きます。
- スクロール・絞り込み後のクリック位置を反映し、ダイアログや移動メニューの表示中は背面のマウス操作を抑止します。

Linux x86_64/ARM64、macOS Intel/Apple Silicon、Windows x64のバイナリを提供します。
ソースからのビルドにはRust 1.88以上が必要です。既存設定を継続利用できます。
