# git-dashboard-tui

**複数リポジトリの次の作業を、ひとつのターミナルで見つける。**

[English](README.md) · [利用マニュアル](https://orapli.github.io/git-dashboard-tui/manual.ja.html) · [操作・設定一覧](docs/reference.ja.md)

未コミットの変更、同期差分、CI失敗を一覧し、差分を確認して普段の作業ツールを開けます。
横断コミット検索、貢献者分析、Worktree一覧にも対応します。

![登録 → 要対応 → 差分 → シェルの操作デモ](docs/img/quick-tour.ja.svg)

このREADMEと公開マニュアルは **main の開発版** を説明します。インストーラが取得するのは
**最新リリース**です。初回登録ガイド、Homeの詳細な取得状態、`O` のツールメニュー、`W` の
横断Worktree一覧は v0.3.1 以降の開発版機能です。[変更履歴](CHANGELOG.md)で区別しています。

## インストール

### 1行インストール（Linux / macOS）

```bash
curl -fsSL https://raw.githubusercontent.com/orapli/git-dashboard-tui/main/install.sh | sh
```

プラットフォームを自動判定し、対応するバイナリをダウンロードして、リリースに公開された
SHA-256 チェックサムと照合したうえで `~/.local/bin`（すでに `PATH` にあり書き込み可能なら
`/usr/local/bin`）へインストールします。`sudo` は一切使いません。

```bash
# インストール先を指定する / バージョンを固定する
curl -fsSL https://raw.githubusercontent.com/orapli/git-dashboard-tui/main/install.sh | INSTALL_DIR=~/bin sh
curl -fsSL https://raw.githubusercontent.com/orapli/git-dashboard-tui/main/install.sh | VERSION=v0.3.1 sh
```

> スクリプトをシェルにパイプするのは、そのスクリプトを信頼することを意味します。本スクリプトは
> 短く依存もないので、事前に目を通すことをおすすめします: [`install.sh`](install.sh) または
> `curl -fsSL https://raw.githubusercontent.com/orapli/git-dashboard-tui/main/install.sh | less`

Windows x64 は [Releases](https://github.com/orapli/git-dashboard-tui/releases/latest) の
`git-dashboard-tui-x86_64-pc-windows-msvc.zip` を展開し、実行ファイルをPATHの通った場所へ配置してください。
Linux（x86_64 / ARM64）、macOS（Intel / Apple Silicon）のアーカイブも同じページにあります。

開発版を使う場合は **Rust 1.88以上** とビルド環境が必要です。

```bash
cargo install --git https://github.com/orapli/git-dashboard-tui --branch main --locked
```

必須: PATH上の `git`。GitHub PR/CI表示には認証済みの `gh`、SSH登録には鍵認証で接続できる `ssh` が必要です。
詳しくは[対応範囲](https://orapli.github.io/git-dashboard-tui/manual.ja.html#platforms)をご覧ください。

## クイックスタート

```bash
git-dashboard-tui
```

1. `A` で作業フォルダを指定し、`Space` で選んで `Enter` で登録。1件なら `a`。
2. Home の `n` で要対応に絞り込み、`Enter` で詳細、Status のファイルを `Enter` で差分表示。
3. `O` でエディタ・lazygit・GitUI、`t` でシェルを開く。Worktree単位の作業は Home の `W`。
4. `?` で画面別ヘルプ、`Esc` で戻る、`q` で終了。

状態確認が中心です。`pull`・`fetch`・`stash apply`・`stash drop` は明示的な操作でリポジトリを変更します。
`pull` はGit設定に従いmerge/rebaseする場合があります。ステージング・コミット・pushは普段のツールで行います。

## 詳しい使い方

| 目的 | 資料 |
|---|---|
| 作業開始から変更確認まで | [目的別の手順](https://orapli.github.io/git-dashboard-tui/manual.ja.html#tasks) |
| 要対応・未確認・CI・キャッシュを理解する | [状態と更新](https://orapli.github.io/git-dashboard-tui/manual.ja.html#freshness) |
| VS Code・Neovim等を設定する | [エディタ設定例](https://orapli.github.io/git-dashboard-tui/manual.ja.html#editor-examples) |
| 認証・表示更新・起動の問題を解決する | [トラブルシューティング](https://orapli.github.io/git-dashboard-tui/manual.ja.html#troubleshooting) |
| キー操作・設定ファイルを調べる | [リファレンス](docs/reference.ja.md) |

## 開発・関連資料

[開発者ガイド](CLAUDE.md) · [ドキュメント生成と検証](docs/README.md) ·
[変更履歴](CHANGELOG.md) · [セキュリティ方針](SECURITY.md) ·
[Issues](https://github.com/orapli/git-dashboard-tui/issues)

生成された[OpenWiki](openwiki/index.md)は補助資料です。コード・テストを正とし、生成時点は
[更新メタデータ](openwiki/.last-update.json)で確認してください。

## ライセンス

[MIT](LICENSE)
