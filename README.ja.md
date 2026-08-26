<div align="center">

# 🚀 git-dashboard-tui

**手元のすべてのリポジトリを、1つのターミナルから — どれ一つ触ることなく。**

[![CI](https://github.com/orapli/git-dashboard-tui/actions/workflows/ci.yml/badge.svg)](https://github.com/orapli/git-dashboard-tui/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-14B8A6)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-1.85%2B%20(2024%20edition)-orange)](https://www.rust-lang.org/)
[![Platforms](https://img.shields.io/badge/platforms-Linux%20%7C%20macOS%20%7C%20Windows-blue)](#インストール)

[クイックスタート](#クイックスタート) · [こんな人に](#こんな人に効きます) · [キーバインド](#キーバインド) · [English README](README.md)

</div>

git-dashboard-tui は、ターミナル向けの**読み取り専用マルチリポジトリ Git ダッシュボード**です。
十数個のリポジトリを登録すれば、どれが未コミットで、どれが遅れていて、どれの CI が落ちていて、
どれが rebase の途中で放置されているかが 1 画面で分かります。そこからシェル・差分・コミットへ
飛んで、実際の作業は普段お使いのツールで行ってください。

```bash
curl -fsSL https://raw.githubusercontent.com/orapli/git-dashboard-tui/main/install.sh | sh
git-dashboard-tui
```

これだけです（Linux / macOS、Rust ツールチェーン不要）。インストーラがプラットフォームを判定し、
チェックサムを検証したうえで導入します。Windows やその他の方法、スクリプトを事前に確認する手順は
[インストール](#インストール)を参照してください。

`a` でリポジトリを追加、`A` でフォルダを走査して配下の Git リポジトリを一括取り込みします。

```text
 Repositories   git-dashboard-tui
┌Repositories (4/4) [Group: All] updated ↓───────────────────────────────────────────────────────────────────────────┐
│  Name                       Branch                         Sync       Dirty    Updated            Path             │
│▸ [frontend] web-frontend    main ⚠MERGE ⚠1conflict         ↑0 ↓0      1        2026-08-26 06:39   /work/web-front… │
│  [backend] billing-service  main                           ↑0 ↓0      1        2026-08-26 06:37   /work/billing-s… │
│  [backend] api-gateway      main                           ↑0 ↓0      0        2026-08-26 06:35   /work/api-gatew… │
│  [infra] infra-terraform    main                           ↑0 ↓0      0        2026-08-26 06:35   /work/infra-ter… │
└────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┘
j/k move  enter open  [/] group  / filter  t shell  P/F pull/fetch all  M members  S search commits  n needs attention
```

## こんな人に効きます

- **把握しきれない数のリポジトリを持っている。** サービスリポジトリを十数個抱えるプラットフォーム/
  インフラエンジニアなら、ブランチ・ahead/behind・未コミット数・オープン PR 数・CI 状態が 1 つの
  リストで見えます。ディレクトリを `cd` して回りながら `git status` を打つ必要はありません。
- **チームを見ていて、詰まっている場所を見つけたい。** `n` を押せば、CI 失敗・未解決コンフリクト・
  誰かが中断したままの merge/rebase があるリポジトリだけに絞り込めます。`M` でリポジトリ横断の
  コントリビューター分析も見られます。
- **clone しないマシンに SSH で入って作業している。** `ssh://host/path` として登録すれば、ローカル
  リポジトリと同じ一覧に並びます。ローカルチェックアウトなしで状態・ログ・差分・貢献者分析が
  動きます。
- **どのリポジトリにあるか分からないコミットを探したい。** `S` は登録済み全リポジトリのコミット
  メッセージを一度に検索し、該当コミットの差分へ直接ジャンプします。

## リポジトリを預けるに足る理由

本ツールは、自分が書いたとは限らないリポジトリに対して `git` を実行します。そのため
**リポジトリの内容を信頼できない入力として扱う**設計になっています。

- **リポジトリは、一覧に載っただけではコードを実行できません。** すべての git 実行時に、
  マシン上でコマンドを起動しうる `.git/config` のキー（`core.fsmonitor` / `core.sshCommand` /
  `uploadpack.packObjectsHook` / `protocol.ext.allow`）を無効化し、さらに
  `--no-ext-diff --no-textconv` を付けて `diff.external` や `textconv` ドライバも発火しないように
  しています。git 自身の `safe.directory` はこの用途を**カバーしません** — 保護対象は
  *他ユーザー所有*のリポジトリだけだからです。
- **git 操作がアプリを固めることはありません。** すべてのコマンドがハードタイムアウト（解析 30 秒 /
  ネットワーク操作 120 秒）付きで動き、出力は 64 MiB で打ち切り、専用スレッドで吸い出すため
  パイプが詰まってデッドロックすることもありません。
- **`ssh://` の接続先は検証されます。** OpenSSH には `--` によるオプション終端がないため、
  フラグとして解釈されうるホストは受け付けません。
- **完全にローカルで動作します。** テレメトリも外部サービスもありません。通信は git が自分の
  リモートと話す分と、インストールされていれば `gh` の分だけです。
- **履歴を書き換えません。そもそも機能がありません。** 書き込み系は `pull` / `fetch` /
  `stash apply` / `stash drop` のみです（[スコープ](#スコープ-このツールがやらないこと)参照）。

脅威モデルと報告方法は [SECURITY.md](SECURITY.md) を参照してください。

## スコープ: このツールが「やらないこと」

git-dashboard-tui は意図的に**観測専用**です。ステージング・コミット・ブランチ作成・checkout・
merge・rebase・cherry-pick・push はありません。何かを変更したくなったら `t` でそのリポジトリの
`$SHELL` に降りるか、Settings の `c` で外部 diff ツールを設定し、普段お使いのツールで作業して
ください。

ステージやコミットまで行えるターミナル UI が必要なら
[lazygit](https://github.com/jesseduffield/lazygit) や
[gitui](https://github.com/gitui-org/gitui) をお使いください。どちらも優れたツールであり、
本ツールとは解く問題が異なります。

## 画面と機能

| 画面 | 表示内容 |
|---|---|
| **Home** | 全リポジトリ: ブランチ、`↑ahead ↓behind`、未コミット数、最終コミット、`[PR:n]`、`✓CI`/`✗CI`、`⚠MERGE`/`⚠REBASE`/`⚠Nconflict` の警告 |
| **Status** (`1`) | 概要 KPI、最近のコミット、ワーキングツリーのファイル一覧 |
| **Commits** (`2`) | git 自身のレーン配色によるコミットグラフ、ブランチ/タグバッジ、base↔target 比較 |
| **Branches / Tags** (`3` `4`) | 作者と件名付きのソート済み一覧。任意の 2 タグを比較可能 |
| **Stash** (`5`) | stash の内容確認・適用・削除 |
| **Contributors** (`6`) | リポジトリ単位の貢献者統計（全期間 / 1週 / 1ヶ月 / 3ヶ月） |
| **Worktrees** (`7`) | 各 worktree のパス・ブランチ・HEAD・lock/prunable 状態。`Enter` でその worktree のシェルへ |
| **Diff** | シンタックスハイライト付き分割/統合差分、hunk 移動、行単位 blame ガター（任意） |
| **Commit Search** (`S`) | 登録済み**全**リポジトリを横断したコミットメッセージ検索 |
| **Global Members** (`M`) | 全リポジトリを集計した貢献者統計（別名マージ対応） |

### リポジトリ横断コミット検索

```text
 Commit Search   git-dashboard-tui
┌Commit Search "timeout" (3)─────────────────────────────────────────────────────────────────────────────────────────┐
│▸ [web-frontend    ] 83ae2c6  2026-08-26  Bob               Raise checkout timeout to 60s                           │
│  [web-frontend    ] bde61da  2026-08-26  Bob               Raise checkout timeout to 45s                           │
│  [web-frontend    ] ec9e778  2026-08-26  Bob               Add checkout timeout                                    │
└────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┘
j/k move  / new search  enter open commit  ? help  esc back  q quit
```

### blame ガター付き差分 (`b`)

未コミット行は、最後に触った人に誤って帰属させるのではなく `Not Committed Yet` として表示されます。

```text
┌  Changed Files (1)──────┐┌  hunks (1/1)──────────┐┌▸ Diff    [f:full b:blame]────────────────────────┐
│▸ [M] src/invoice.rs     ││▸ #1   L10    /// Total││0000000 Not Committed…│ +/// Total including tax, │
│                         ││                       ││ce832db Carol         │  pub fn total(items: &[Ite│
│                         ││                       ││ce832db Carol         │      let sub = subtotal(it│
│                         ││                       ││ce832db Carol         │      sub + tax(sub, rate) │
└─────────────────────────┘└───────────────────────┘└──────────────────────────────────────────────────┘
hunk 1/1  n/p hunk  tab pane  [/] file  w ignore ws  f full file  b blame  t shell  ? help  esc back  q
```

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
curl -fsSL https://raw.githubusercontent.com/orapli/git-dashboard-tui/main/install.sh | VERSION=v0.2.1 sh
```

> スクリプトをシェルにパイプするのは、そのスクリプトを信頼することを意味します。本スクリプトは
> 短く依存もないので、事前に目を通すことをおすすめします: [`install.sh`](install.sh) または
> `curl -fsSL https://raw.githubusercontent.com/orapli/git-dashboard-tui/main/install.sh | less`

### ビルド済みバイナリを手動で

単体で動くバイナリ 1 つだけです。お使いのプラットフォームを選んでください:

```bash
# Linux x86_64
curl -sSL https://github.com/orapli/git-dashboard-tui/releases/latest/download/git-dashboard-tui-x86_64-unknown-linux-gnu.tar.gz | tar xz

# Linux ARM64
curl -sSL https://github.com/orapli/git-dashboard-tui/releases/latest/download/git-dashboard-tui-aarch64-unknown-linux-gnu.tar.gz | tar xz

# macOS (Apple Silicon)
curl -sSL https://github.com/orapli/git-dashboard-tui/releases/latest/download/git-dashboard-tui-aarch64-apple-darwin.tar.gz | tar xz

# macOS (Intel)
curl -sSL https://github.com/orapli/git-dashboard-tui/releases/latest/download/git-dashboard-tui-x86_64-apple-darwin.tar.gz | tar xz
```

そのあと `PATH` の通った場所へ移動します:

```bash
chmod +x git-dashboard-tui
sudo mv git-dashboard-tui /usr/local/bin/    # ~/.local/bin など PATH 上ならどこでも可
```

**Windows (x64)**: [Releases](https://github.com/orapli/git-dashboard-tui/releases/latest) から
`git-dashboard-tui-x86_64-pc-windows-msvc.zip` をダウンロードし、`git-dashboard-tui.exe` を展開してください。

> macOS では、署名されていないバイナリの初回起動が Gatekeeper にブロックされることがあります。
> **システム設定 → プライバシーとセキュリティ** から許可するか、
> `xattr -d com.apple.quarantine git-dashboard-tui` を実行してください。

全バイナリは [Releases](https://github.com/orapli/git-dashboard-tui/releases) ページにあります。

### cargo で入れる（ソースからビルド、クローン不要）

```bash
# Rust 1.85+ (2024 edition) が必要です
cargo install --git https://github.com/orapli/git-dashboard-tui --locked
```

### クローンから（開発用）

```bash
git clone https://github.com/orapli/git-dashboard-tui
cd git-dashboard-tui
cargo install --path . --locked
```

### 動作要件

| | |
|---|---|
| **必須** | `PATH` の通った `git` |
| **任意** | 認証済みの [`gh`](https://cli.github.com/) — `[PR:n]` と `✓CI`/`✗CI` バッジが有効になります。無い場合はその列が出ないだけで、他の動作は変わりません |
| **任意** | `ssh` クライアント（`ssh://` リポジトリ用。鍵認証のみ — `BatchMode=yes` によりパスワード入力を求めません） |
| **検証環境** | Linux / macOS / Windows で push ごとに CI 実行 |

## クイックスタート

```bash
git-dashboard-tui
```

そのあとは:

1. `a` — パスを指定してリポジトリを 1 つ追加（`Tab` で補完）
2. `A` — またはフォルダを走査して配下の Git リポジトリを一括取り込み
3. `Enter` — リポジトリを開く。`1`〜`7` でタブ切り替え
4. `t` — 実際に何か変更したくなったら `$SHELL` に降りる
5. `?` — 画面ごとのキーバインドヘルプ

## キーバインド

### 全体共通

| キー | 動作 |
|---|---|
| `q` / `Ctrl+C` | 終了 |
| `Esc` / `h` / `←` | 前の画面へ戻る / 絞り込み解除 |
| `?` | ヘルプ表示（元の画面に戻る） |
| `/` | 現在の一覧を絞り込み |
| `j` / `k`（`↓` / `↑`） | 選択を移動 |
| `g` / `G` | 一覧の先頭 / 末尾へ |
| `t` | 現在のリポジトリで `$SHELL` を開く<br>（Contributors タブでは `t` はメンバー状態の切替） |
| **マウスホイール** | 一覧・コミットログ・差分・ヘルプ画面のスクロール |
| **マウスクリック** | タブ切り替え・項目選択 |
| **列ヘッダをクリック** | Home 一覧をその列でソート。同じ列を再度クリックすると昇降順が反転 |
| **一覧の行をクリック** | 選択。選択済みの行をもう一度クリックすると開く |
| **`[ ]` マーカーをクリック** | Commits / Tags で比較の基準→対象を選択（`space` と同じ）。選択済みの行を再クリックで解除 |

### 1. Home（マルチリポジトリ画面）

| キー | 動作 |
|---|---|
| `Enter` | 選択したリポジトリを開く |
| `a` / `A` | パス指定で追加 / **Repository Finder**（フォルダ走査）を起動 |
| `d` | 選択したリポジトリを削除 |
| `e` | 表示名を変更 |
| `o` | ソート順を切替 — 名前・ブランチ・同期・変更数・更新日時の各↑↓。ソート中の列はヘッダに ▲ / ▼ で表示され、クリックでも切替可能 |
| `[` / `]` | グループフィルタを切替 |
| `n` | **要対応フィルタ**（CI 失敗 / コンフリクト / 中断中の操作）の切替 |
| `S` | **リポジトリ横断コミット検索** |
| `M` | **全リポジトリ横断メンバー分析**を開く |
| `p` / `f` | 選択リポジトリの `git pull` / `git fetch`。実行中はそのリポジトリの同期列にスピナーと操作名が表示され、タイトルバーに残りジョブ数が出ます |
| `P` / `F` | 絞り込み中の全リポジトリへ一括 `pull` / `fetch` |
| `r` | 全リポジトリ行を再読み込み |
| `t` / `T` | 選択リポジトリで `$SHELL` を開く |
| `s` | 設定画面を開く |

### 2. リポジトリ詳細（`1`〜`7`）

| タブ | キー | 動作 |
|---|---|---|
| **タブ** | `1` - `7` / `Tab` / `[` `]` | Status, Commits, Branches, Tags, Stash, Contributors, Worktrees を切替 |
| **(全タブ共通)** | `p` / `f` | このリポジトリの `git pull` / `git fetch` |
| | `r` | リポジトリデータを再読み込み |
| | `T` | リポジトリルートで `$SHELL` を開く |
| **Status** (`1`) | `Enter` | ワーキングツリーのファイル差分を開く |
| **Commits** (`2`) | `Space` | 比較用の base/target コミットを選択 |
| | `Enter` | コミット差分 / 比較を開く |
| | `i` | 内蔵 TUI 差分を強制的に使う |
| **Branches** (`3`) | `Enter` | ブランチの oneline ログを表示 |
| **Tags** (`4`) | `Space` / `Enter` | タグ範囲を選択 / タグ間比較 |
| **Stash** (`5`) | `Enter` / `a` / `d` | stash 差分を確認 / 適用 / 削除 |
| **Contributors** (`6`) | `w` | 期間を切替（**全期間** / **1週** / **1ヶ月** / **3ヶ月**） |
| | `Space` / `t` | メンバーの在籍状態を切替 |
| | `m` | 在籍メンバーのみ表示 |
| **Worktrees** (`7`) | `Enter` | **選択した worktree 内で `$SHELL` を開く**<br>（`t` / `T` 単体はリポジトリルートを開きます） |

### 3. 差分画面

| キー | 動作 |
|---|---|
| `Tab` / `l` / `→` | フォーカスを前方循環: **ファイル** → **hunk** → **差分本文** |
| `Shift+Tab` / `h` / `←` | フォーカスを後方循環 |
| `n` / `p`（`N`） | 次 / 前の変更 hunk へ |
| `[` / `]` | 前 / 次の変更ファイルへ |
| `g` / `G` | 差分の先頭 / 末尾へ |
| `PageUp` / `PageDown` / `Space` | 差分本文をページ単位でスクロール |
| `w` | **空白無視**の切替 |
| `f` | **全文表示**の切替 |
| `b` | **blame ガター**（行ごとの短縮ハッシュ + 作者）の切替 |
| `Enter` | 選択ファイルを読み込み（ファイルペイン） / hunk へジャンプ（hunk ペイン） |
| `t` / `T` | リポジトリルートでターミナルを開く |
| `Esc` | リポジトリ画面へ戻る |

### 4. 設定（`s`）

| キー | 動作 |
|---|---|
| `Tab` / `1` / `2` | **リポジトリ**タブと**メンバー**タブを切替 |
| `a` | リポジトリ / メンバーを追加（タブによる） |
| `A` | **Repository Finder** を起動（リポジトリタブ） |
| `g` | リポジトリのグループを編集（リポジトリタブ） |
| `e` | リポジトリ名の変更 / メンバー別名の編集 |
| `d` | 選択したリポジトリ / メンバーを削除 |
| `Space` / `t` | メンバーの在籍状態を切替（メンバータブ） |
| `c` | 外部 diff ツールを設定（空にすると内蔵ビューア） |
| `l` | 言語を切替（English / 日本語） |
| `i` | **自動更新**の間隔を切替（オフ / 30秒 / 1分 / 5分） |
| `T` | **テーマ**を切替（Catppuccin Mocha / Latte） |

### 5. コミット検索（Home で `S`）

| キー | 動作 |
|---|---|
| `/` | 新しい検索を開始 |
| `Enter` | 選択コミットの差分を開く（該当リポジトリへ移動） |
| `Esc` | Home へ戻る |

### 6. 横断メンバー（Home で `M`）

| キー | 動作 |
|---|---|
| `Tab` / `h` / `l` | メンバー一覧とリポジトリ一覧のペインを切替 |
| `Enter` | 選択したリポジトリへ移動 |
| `Space` / `t` | メンバーの在籍 / 非在籍を切替 |
| `m` | 在籍メンバーのみに絞り込み |
| `T` | 選択リポジトリで `$SHELL` を開く |
| `/` | メンバー / リポジトリを検索 |

### 7. Repository Finder（Home で `A`）

| キー | 動作 |
|---|---|
| `Space` | ハイライト中のリポジトリの選択を切替 |
| `a` | 全選択 / 全解除 |
| `Enter` | 選択したリポジトリを一括登録 |
| `r` | 走査対象フォルダを変更 |
| `/` | 検出されたリポジトリを絞り込み |
| `Esc` / `q` | キャンセル |

## 設定ファイル

設定はプレーンな JSON で、アトミックに書き込まれ、`git-dashboard` GUI アプリと共有されます。

- **macOS**: `~/Library/Application Support/com.git-dashboard.git-dashboard/`
- **Linux**: `~/.config/git-dashboard/`
- **Windows**: `%APPDATA%\git-dashboard\git-dashboard\config\`

| ファイル | 内容 |
|---|---|
| `config.json` | 登録済みリポジトリ（名前・パス・グループ） |
| `members.json` | チームメンバーと、そこへ統合するコミット作者の別名 |
| `prefs.json` | 言語・テーマ・差分トグル・ソート順・自動更新間隔・外部 diff コマンド |
| `tech_rules.json` | マニフェストファイルから言語/フレームワークのバージョンを検出するルール |
| `cache/` | Home 行とリポジトリ解析結果のキャッシュ。更新が終わるまで空欄にせず、前回の内容を即座に表示するために使います。削除しても次回更新時に再生成されます |

ほとんどの設定はアプリ内（`s` で設定画面）から変更できます。手で編集する必要はありません。

### 外部 diff ツールを使う

設定画面で `c` を押すと、内蔵ビューアの代わりに別ツールへ差分を渡せます。コマンドには
`{path}` `{range}` `{base}` `{target}` `{file}` のプレースホルダを含められます。空にすると
内蔵ビューアに戻ります。プログラムは `PATH` から解決するか絶対パスで指定してください —
相対パスは意図的に拒否されます。子プロセスは**表示中のリポジトリ**を作業ディレクトリとして
起動するため、`./tool` のような指定はそのリポジトリ同梱のバイナリを実行してしまうからです。

### SSH 越しのリモートリポジトリ

`ssh://host/absolute/path` の形式で登録します（host は `~/.ssh/config` のエイリアスでも可）。
状態・ログ・差分・貢献者分析は、更新のたびに 1 本の多重化した SSH 接続で実行されます。認証は
鍵認証のみで、`BatchMode=yes` によりパスワード待ちで止まることはありません。ローカルの
`$SHELL` を開く機能だけは、これらのリポジトリでは利用できません。

## 関連ドキュメント

- **[変更履歴](CHANGELOG.md)** — リリースごとの変更点
- **[セキュリティポリシー](SECURITY.md)** — 脅威モデルと脆弱性の報告方法
- **[エンジニアリング Wiki](openwiki/index.md)** — 生成されたアーキテクチャ資料。
  [ワーカー/キューモデル](openwiki/application/background-work.md)、
  [git 実行エンジン](openwiki/git/engine.md) ほか
- **[開発者ガイド](CLAUDE.md)** — コマンド・モジュール構成・実装ルール
- **[Issues](https://github.com/orapli/git-dashboard-tui/issues)** — バグ報告・機能要望

## 開発

```bash
cargo fmt              # フォーマット
cargo clippy --all-targets -- -D warnings
cargo test             # ユニット + 実リポジトリを使う統合テスト
cargo run              # デバッグビルドで実行
```

CI では push ごとに Linux / macOS / Windows でフォーマット・Clippy（`-D warnings`）・全テストを
実行します。`fmt` や `clippy` を通らないコミットは CI で落ちます。

```
src/
├── app/        # 状態機械、キー/マウス処理、バックグラウンドワーカー
├── git/        # git コマンドの実行と出力パース
├── ui.rs       # Ratatui による描画
├── syntax.rs   # 軽量シンタックストークナイザ
├── colors.rs   # Catppuccin パレット
└── config.rs   # 設定の読み書き（必ず write_atomic 経由）
```

ワーカーモデル・git 実行境界・差分パイプラインを変更する前に、
[エンジニアリング Wiki](openwiki/index.md) に目を通してください。

## ライセンス

[MIT](LICENSE)
