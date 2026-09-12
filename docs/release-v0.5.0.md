# v0.5.0 — Faster refreshes, an honest table, and a way out of the terminal

Refreshing a dashboard of 30 repositories went from about 14 seconds to under a
quarter of a second, the Home table stopped reporting confident answers it does not
have, and the dashboard can now be read by a script as well as by you.

## Faster

- A repository analysis no longer pays a fixed 25 ms sleep per git call, no longer
  computes statistics the Home row never displays, and no longer shares one worker
  thread with everything you do. Measured by driving the real binary under a PTY,
  before and after, on the same fixtures: 30 repositories cold refresh **13.98 s →
  0.22 s**, and pressing `r` then immediately opening a repository, timed until its
  commit list is populated, **14.61 s → 0.24 s**. Single runs on fixtures with no
  GitHub remotes, so they measure local git work; a cold refresh of repositories with
  GitHub remotes is still bounded by `gh`.

## Says what it does not know

- `↑? ↓?` where a branch has no upstream, instead of the `↑0 ↓0` that looked exactly
  like being in sync.
- A registration that cannot be read is its own state with its reason, instead of a
  row stuck on `…` forever — or, for a directory that is not a repository, a row that
  looked perfectly healthy. A repository whose index is corrupt is one of these, not a
  clean working tree.
- `2M 5?` splits tracked changes from untracked files, instead of one total in which
  five build artefacts and two real edits look the same.
- The panel reports when the remote data was last fetched, separately from when the
  working tree was last read.
- CI is the latest run **on the branch you are on**, not the repository's newest run
  anywhere, and every way of not having a CI answer has its own mark.
- Pull requests waiting on your review, and changes requested on your branch's pull
  request, appear on the row and in the `n` filter.

## Readable from outside

- `git-dashboard-tui PATH` starts focused on a repository without registering it.
- `--json` prints a status snapshot and exits, with no terminal and no network calls:
  CI and pull-request fields come from the cache the dashboard already wrote and say
  when they are absent or stale.

## Hands off better

- Command templates take `{path}`, `{file}`, `{line}`, `{branch}` and `{hash}`, so the
  editor opens what you were reading rather than the repository root.
- Up to six user-defined commands in the `O` menu, edited with `x`.
- `y` copies the identifier for the current selection.
- Opening a repository lands on Status when there is something in the working tree.
- Commit search takes `author:`, `path:`, `since:` and `until:`, and says when results
  were truncated instead of quietly hiding them.
- `?` leads with the keys for the screen you pressed it on.

Pre-built binaries cover Linux x86_64/ARM64, macOS Intel/Apple Silicon and Windows x64.
Verify downloaded archives with `SHA256SUMS`. On Linux/macOS, install the latest release with:

```bash
curl -fsSL https://raw.githubusercontent.com/orapli/git-dashboard-tui/main/install.sh | sh
```

Building from source requires Rust 1.88 or later. Existing configuration and caches
remain compatible: a row restored from a v0.4.1 cache reports what it does not know
rather than guessing, and refreshes into the new detail.

# 日本語

30リポジトリのダッシュボードの更新が約14秒から0.25秒未満になり、Home の表が
「確信のない答」を出さなくなり、スクリプトからも読めるようになりました。

## 速くなりました

- リポジトリ1件の解析が、git 呼び出しごとの固定25msの待機を払わなくなり、Home 行が
  表示しない統計を計算しなくなり、操作用のワーカースレッドを共有しなくなりました。
  実バイナリをPTYで操作し、同一条件で前後を測定した結果、30リポジトリのコールドな
  全体更新が **13.98秒 → 0.22秒**、`r` の直後にリポジトリを開いてコミット一覧が
  埋まるまでが **14.61秒 → 0.24秒** です。GitHubリモートを持たないフィクスチャでの
  単発測定で、ローカルのGit処理を測ったものです。GitHubリモートがある場合の初回更新は
  引き続き `gh` の応答時間に左右されます。

## 「分からない」と言うようになりました

- upstream がないブランチは `↑0 ↓0`（同期済みと見分けがつかない）ではなく `↑? ↓?`。
- 読み取れない登録は、`…` のまま止まる行でも、健全に見える行でもなく、理由を持つ
  ひとつの状態になりました。インデックスが壊れたリポジトリもこれに含まれます。
- `2M 5?` が追跡対象の変更と未追跡ファイルを分けます。ビルド成果物5件と実際の編集2件が
  同じ「7」に見えることはなくなりました。
- パネルが、作業ツリーを最後に読んだ時刻とは別に、リモート情報の取得時刻を表示します。
- CI は「いま居るブランチ」の最新実行です。答が得られない場合はそれぞれ専用の印を出します。
- 自分のレビュー待ちのPRと、自分のブランチのPRに来た変更要求が、行と `n` に出ます。

## 外から読めます

- `git-dashboard-tui PATH` は登録せずにそのリポジトリを選択した状態で起動します。
- `--json` は端末を使わずネットワークにも接続せず、状態をJSONで出力して終了します。
  CI・PR はダッシュボードが書いたキャッシュのみを読み、未取得や古い場合はそう明示します。

## 受け渡しが良くなりました

- コマンドテンプレートが `{path}` `{file}` `{line}` `{branch}` `{hash}` に対応し、
  エディタがリポジトリのルートではなく、読んでいたファイルを開きます。
- `O` メニューにユーザー定義コマンドを6件まで登録できます（`x` で編集）。
- `y` で選択中の項目の識別子をコピーします。
- 作業ツリーに変更があるリポジトリは Status タブで開きます。
- コミット検索が `author:` `path:` `since:` `until:` に対応し、打ち切った場合は明示します。
- `?` は押した画面のキーを先頭に表示します。

Linux x86_64/ARM64、macOS Intel/Apple Silicon、Windows x64のバイナリを提供します。
ソースからのビルドにはRust 1.88以上が必要です。既存の設定とキャッシュはそのまま使えます。
