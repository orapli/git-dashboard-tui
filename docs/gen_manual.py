#!/usr/bin/env python3
"""Generate the English and Japanese manuals from one source.

Both languages come out of the same structure on purpose. The README pair in
this repository has drifted twice — a keybinding corrected in one file and not
the other — and a manual is longer and easier to let rot. Here a section that
gains a paragraph in English cannot silently keep the old Japanese: the two
sit next to each other in the same tuple.

    python3 docs/gen_manual.py [--out docs]
"""

from __future__ import annotations

import argparse
import html
from pathlib import Path

from manual_tasks import sections as task_sections

EN, JA = 0, 1


def t(en: str, ja: str) -> tuple[str, str]:
    return (en, ja)


# --- content ---------------------------------------------------------------
# Each section: (id, title, blocks). A block is one of:
#   ("p", text)              paragraph
#   ("shot", stem, caption)  screenshot with caption
#   ("keys", [(key, desc)])  keybinding table
#   ("cols", [(name, desc)]) definition table
#   ("code", text)           preformatted block
#   ("note", text)           aside
#   ("warn", text)           warning aside

SECTIONS = [
    (
        "what",
        t("What this is", "これは何か"),
        [
            ("p", t(
                "A terminal dashboard for the repositories you already have checked out. "
                "It answers the question you ask before you start work — which of these has "
                "uncommitted changes, which is behind, which did I leave half-way through a "
                "merge — without cd-ing through a dozen directories.",
                "手元にチェックアウト済みのリポジトリを一覧するターミナルダッシュボードです。"
                "作業を始める前に知りたいこと — どれに未コミットの変更があるか、どれが遅れているか、"
                "どれを merge の途中で放置したか — を、ディレクトリを渡り歩かずに把握するためのものです。")),
            ("p", t(
                "Status viewing is observation-first. Explicit pull, fetch, stash apply and "
                "stash drop actions can change repositories. Pull follows your Git configuration "
                "and may merge or rebase. Use your usual tools for staging, committing and pushing.",
                "状態確認を中心としたツールです。明示的に実行する pull・fetch・stash apply・stash drop は"
                "リポジトリを変更します。pull は Git 設定に従って merge / rebase を行う場合があります。"
                "ステージング・コミット・push は普段のツールで行ってください。")),
            ("shot", "home", t(
                "Five repositories. api-gateway is clean, billing-service has two uncommitted "
                "changes, infra-terraform is two commits behind its upstream, and web-frontend "
                "was left in the middle of a merge with one unresolved conflict.",
                "5つのリポジトリ。api-gateway はクリーン、billing-service に未コミットが2件、"
                "infra-terraform は upstream から2コミット遅れ、web-frontend は merge 途中で"
                "未解決のコンフリクトが1件残っています。")),
        ],
    ),
    (
        "install",
        t("Installing", "インストール"),
        [
            ("p", t(
                "The installer detects your platform, downloads the matching release binary, "
                "verifies its SHA-256 against the checksum published with the release, and "
                "installs to <code>~/.local/bin</code> (or writable <code>/usr/local/bin</code> already on PATH). It never uses sudo.",
                "インストーラはプラットフォームを判定し、対応するリリースバイナリを取得し、"
                "リリースと同時に公開される SHA-256 と照合し、<code>~/.local/bin</code>（PATH上にあり書込可能なら <code>/usr/local/bin</code>）に配置します。"
                "sudo は一切使いません。")),
            ("code", t(
                "curl -fsSL https://raw.githubusercontent.com/orapli/git-dashboard-tui/main/install.sh | sh",
                "curl -fsSL https://raw.githubusercontent.com/orapli/git-dashboard-tui/main/install.sh | sh")),
            ("p", t(
                "Piping a script into a shell means trusting it, so read it first if you "
                "prefer — it is short and has no dependencies. "
                "<code>INSTALL_DIR</code> and <code>VERSION</code> override the defaults. "
                "Pre-built binaries cover Linux (x86_64, ARM64), macOS (Apple Silicon, Intel) "
                "and Windows (x64); <code>cargo install --git</code> works too.",
                "スクリプトをシェルに流し込むのはそれを信頼するということなので、"
                "気になる場合は先に読んでください（短く、依存もありません）。"
                "<code>INSTALL_DIR</code> と <code>VERSION</code> で既定値を上書きできます。"
                "ビルド済みバイナリは Linux (x86_64, ARM64)、macOS (Apple Silicon, Intel)、"
                "Windows (x64) を用意しています。<code>cargo install --git</code> でも導入できます。")),
            ("p", t(
                "Requirements: <code>git</code> on your PATH. The GitHub columns additionally "
                "need authenticated <code>gh</code>. If unavailable, GitHub status is unverified; local Git views still work.",
                "必要なもの: PATH の通った <code>git</code>。GitHub 連携列を使う場合は認証済みの "
                "<code>gh</code> CLI が追加で必要です。利用できない場合はGitHub情報が未確認となりますが、ローカルGit表示は使えます。")),
        ],
    ),
    (
        "first-run",
        t("First run", "最初の起動"),
        [
            ("p", t(
                "Start it with no arguments. Press <kbd>A</kbd>, enter the folder to scan, "
                "then select repositories and register them with <kbd>Enter</kbd>. "
                "Discovery runs in the background and fills the list progressively. <kbd>Esc</kbd>/<kbd>q</kbd> cancels a running scan; <kbd>Enter</kbd> stops scanning and imports the repositories selected so far. An empty path cancels before scanning. Use <kbd>a</kbd> to add one repository.",
                "引数なしで起動し、<kbd>A</kbd> で走査するフォルダを入力します。"
                "検出一覧でリポジトリを選び、<kbd>Enter</kbd> でまとめて登録します。"
                "検出はバックグラウンドで進み、結果を順次表示します。走査中は <kbd>Esc</kbd>/<kbd>q</kbd> で中止、<kbd>Enter</kbd> で走査を止めて検出済みの選択分を登録します。空入力は走査開始前の取消です。1件だけなら <kbd>a</kbd> を使います。")),
            ("p", t(
                "After registration, a short guide introduces <kbd>n</kbd> (needs attention), "
                "<kbd>Enter</kbd> (details), <kbd>t</kbd> (shell), and <kbd>?</kbd> (help). "
                "Dismiss it with <kbd>Esc</kbd>; it stays dismissed on subsequent launches.",
                "登録後の案内には <kbd>n</kbd>（要対応）、<kbd>Enter</kbd>（詳細）、"
                "<kbd>t</kbd>（シェル）、<kbd>?</kbd>（ヘルプ）を表示します。"
                "<kbd>Esc</kbd> で閉じると、次回からは表示しません。")),
            ("p", t(
                "Rows fill in as each repository is analysed, and the result is cached — so the "
                "next launch shows the dashboard populated straight away rather than a screen "
                "of placeholders while it re-reads everything.",
                "各リポジトリの解析が終わるごとに行が埋まり、結果はキャッシュされます。"
                "そのため次回の起動では、読み直しを待つ間の空欄ではなく、"
                "前回の内容が即座に表示されます。")),
            ("shot", "progress", t(
                "Work in flight. The repository being refreshed shows a spinner in place of the "
                "value about to change, and the title bar counts the jobs still running — from "
                "any screen, since work started here keeps going while you look elsewhere.",
                "処理中の様子。更新中のリポジトリは、これから変わる値の位置にスピナーを表示し、"
                "タイトルバーには残りジョブ数が出ます。ここで始めた処理は別の画面に移っても続くため、"
                "この表示は全画面共通です。")),
        ],
    ),
    (
        "dashboard",
        t("The dashboard", "ダッシュボード"),
        [
            ("p", t(
                "Every row is one repository. The colours carry the meaning, so the table is "
                "readable at a glance rather than something you parse column by column.",
                "1行が1リポジトリです。意味は色が担っているので、"
                "列を順に読み解くのではなく一目で状況をつかめます。")),
            ("cols", [
                (t("Name", "名前"), t(
                    "The alias you gave it, prefixed by its group in yellow.",
                    "設定した表示名。前に黄色でグループ名が付きます。")),
                (t("Branch", "ブランチ"), t(
                    "Current branch, followed by badges: <b class='red'>⚠MERGE</b>, "
                    "<b class='red'>⚠REBASE</b>, <b class='red'>⚠CHERRY-PICK</b> or "
                    "<b class='red'>⚠REVERT</b> when an operation was left unfinished, and a "
                    "conflict count. Badges are drawn first so they survive if the column is "
                    "too narrow for everything.",
                    "現在のブランチと、その後に付くバッジ。操作が中断されたままの場合に "
                    "<b class='red'>⚠MERGE</b> / <b class='red'>⚠REBASE</b> / "
                    "<b class='red'>⚠CHERRY-PICK</b> / <b class='red'>⚠REVERT</b> と、"
                    "コンフリクト件数が出ます。列幅が足りない場合でも残るよう、バッジを先に描画しています。")),
                (t("Sync", "同期"), t(
                    "Commits ahead of and behind the upstream, <b class='yellow'>yellow</b> when "
                    "either is non-zero. Replaced by a spinner while a pull or fetch runs.",
                    "upstream に対する先行・遅れのコミット数。どちらかが 0 でなければ"
                    "<b class='yellow'>黄色</b>になります。pull / fetch 実行中はスピナーに変わります。")),
                (t("Dirty", "未コミット"), t(
                    "Uncommitted changes — <b class='green'>green</b> at zero, "
                    "<b class='red'>red</b> otherwise.",
                    "未コミットの変更数。0 なら<b class='green'>緑</b>、それ以外は<b class='red'>赤</b>。")),
                (t("Last commit", "最終コミット"), t(
                    "Date of the most recent commit across all refs.",
                    "全 ref のうち最新コミットの日時。")),
                (t("PR / CI", "PR / CI"), t(
                    "Open pull requests and the latest CI conclusion, for repositories with a "
                    "GitHub remote and an authenticated <code>gh</code>. A failing, cancelled, "
                    "timed-out or action-required run counts as needing attention.",
                    "GitHub リモートがあり <code>gh</code> が認証済みの場合の、"
                    "オープン中の PR 数と最新の CI 結果。failure / cancelled / timed_out / "
                    "action_required は「要対応」として扱われます。")),
            ]),
            ("shot", "home-attention", t(
                "<kbd>n</kbd> narrows the list to what actually needs a response: a failing CI "
                "run, an unresolved conflict, or an interrupted operation. Uncommitted changes "
                "and being behind are normal working state, so they are deliberately not "
                "included.",
                "<kbd>n</kbd> で「本当に対応が要るもの」だけに絞り込みます — CI 失敗、未解決の"
                "コンフリクト、中断された操作。未コミットや遅れは通常の作業状態なので、"
                "意図的に含めていません。")),
            ("shot", "home-sorted", t(
                "Click a column header to sort by it; click the same one again to reverse. The "
                "sorted column carries a ▲ or ▼. <kbd>o</kbd> cycles the same orders from the "
                "keyboard. Repositories whose row has not loaded yet sort last, so the order "
                "does not churn while the dashboard fills in.",
                "列ヘッダをクリックするとその列で並び替え、同じ列を再クリックで昇降が反転します。"
                "並び替え中の列には ▲ / ▼ が付きます。<kbd>o</kbd> でも同じ順序を切り替えられます。"
                "まだ読み込まれていない行は末尾に置かれるため、"
                "ダッシュボードが埋まっていく間に並びが乱れません。")),
            ("keys", [
                (t("Enter", "Enter"), t("Open the selected repository", "選択したリポジトリを開く")),
                (t("a / A", "a / A"), t("Add by path / scan a folder and bulk-add", "パスで追加 / フォルダを走査して一括追加")),
                (t("d / e", "d / e"), t("Remove / rename the alias", "削除 / 表示名の変更")),
                (t("[ / ]", "[ / ]"), t("Cycle the group filter", "グループフィルタを切り替え")),
                (t("/", "/"), t("Filter by name, path or group", "名前・パス・グループで絞り込み")),
                (t("n", "n"), t("Toggle the needs-attention filter", "要対応フィルタの切り替え")),
                (t("C", "C"), t("Open repository-wide latest CI run", "リポジトリ全体の最新CI実行を開く")),
                (t("O", "O"), t("Open work tools", "作業ツールの起動メニュー")),
                (t("W", "W"), t("Browse local worktrees across repositories", "ローカルWorktreeを横断表示")),
                (t("o", "o"), t("Cycle the sort order", "並び順を切り替え")),
                (t("p / f", "p / f"), t("Pull / fetch the selected repository", "選択リポジトリを pull / fetch")),
                (t("P / F", "P / F"), t("Pull / fetch every filtered repository", "絞り込み中の全リポジトリを pull / fetch")),
                (t("S / M", "S / M"), t("Cross-repository commit search / contributors", "リポジトリ横断のコミット検索 / メンバー分析")),
                (t("t", "t"), t("Open a shell in the repository", "そのリポジトリでシェルを開く")),
                (t("r / s", "r / s"), t("Reload every row / open Settings", "全行の再読み込み / 設定画面")),
            ]),
        ],
    ),
    (
        "repository",
        t("Inside a repository", "リポジトリの中"),
        [
            ("p", t(
                "Seven tabs, reachable with <kbd>1</kbd>–<kbd>7</kbd>, <kbd>Tab</kbd>, or by "
                "clicking them. On a narrow terminal the labels shorten rather than the last "
                "tabs disappearing.",
                "7つのタブがあり、<kbd>1</kbd>〜<kbd>7</kbd>・<kbd>Tab</kbd>・クリックで移動できます。"
                "端末幅が狭い場合は、末尾のタブが消えるのではなくラベルが短縮されます。")),
            ("shot", "repo-status", t(
                "Status — the summary, the technology detected from manifest files, and the "
                "working-tree changes. <kbd>Enter</kbd> on a file opens its diff.",
                "Status — 概要、マニフェストから検出した技術スタック、作業ツリーの変更。"
                "ファイル上で <kbd>Enter</kbd> を押すと差分が開きます。")),
            ("shot", "repo-commits", t(
                "Commits — the graph keeps git's own per-lane colours, so concurrent branches "
                "stay visually distinct. Refs are shown as badges: <b class='green'>(main)</b> "
                "for the current branch, <b class='blue'>[hotfix/timeout]</b> for others.",
                "Commits — グラフは git 自身のレーン色をそのまま使うため、"
                "並行するブランチが視覚的に区別できます。ref はバッジ表示で、"
                "現在のブランチが <b class='green'>(main)</b>、それ以外が <b class='blue'>[hotfix/timeout]</b> です。")),
            ("shot", "repo-compare", t(
                "Pick any two commits to compare: click the <code>[ ]</code> at the start of a "
                "row, or press <kbd>space</kbd>. The first becomes the base <b class='green'>[B]</b>, "
                "the second the target <b class='yellow'>[T]</b>, and <kbd>Enter</kbd> diffs the "
                "range. Clicking a marked row again clears it.",
                "任意の2コミットを比較できます。行頭の <code>[ ]</code> をクリックするか "
                "<kbd>space</kbd> を押してください。1つ目が基準 <b class='green'>[B]</b>、"
                "2つ目が対象 <b class='yellow'>[T]</b> になり、<kbd>Enter</kbd> でその範囲の差分を開きます。"
                "選択済みの行を再クリックすると解除されます。")),
            ("shot", "repo-branches", t(
                "Branches — local and remote, with the last commit's author, date and subject. "
                "<kbd>Enter</kbd> shows that branch's log.",
                "Branches — ローカルとリモート、最終コミットの作者・日時・件名付き。"
                "<kbd>Enter</kbd> でそのブランチのログを表示します。")),
            ("shot", "repo-tags", t(
                "Tags — same marking as Commits: pick two and compare the range between them.",
                "Tags — Commits と同じ選択方法で、2つ選んでその間の差分を比較できます。")),
            ("shot", "repo-stash", t(
                "Stash — <kbd>Enter</kbd> shows the diff, <kbd>a</kbd> applies, <kbd>d</kbd> "
                "drops after a confirmation.",
                "Stash — <kbd>Enter</kbd> で差分、<kbd>a</kbd> で適用、<kbd>d</kbd> は確認のうえ削除します。")),
            ("shot", "repo-contributors", t(
                "Contributors — commit counts per person, with aliases merged into one identity "
                "using the member list you configure. <kbd>w</kbd> cycles the time span.",
                "Contributors — 人ごとのコミット数。設定したメンバー一覧に従って"
                "別名を1人に統合します。<kbd>w</kbd> で集計期間を切り替えられます。")),
            ("shot", "repo-worktrees", t(
                "Worktrees — every linked worktree with its branch, HEAD and lock state. "
                "<kbd>Enter</kbd> opens a shell there.",
                "Worktrees — 各ワークツリーのブランチ・HEAD・ロック状態。"
                "<kbd>Enter</kbd> でそのワークツリーでシェルを開きます。")),
        ],
    ),
    (
        "diff",
        t("Reading a diff", "差分を読む"),
        [
            ("p", t(
                "Three panes: the changed files, the hunks in the selected file, and the diff "
                "itself. <kbd>Tab</kbd> moves between them, <kbd>n</kbd> and <kbd>p</kbd> jump "
                "hunk to hunk.",
                "3つのペインで構成されます — 変更ファイル、選択ファイル内のハンク、差分本体。"
                "<kbd>Tab</kbd> でペイン移動、<kbd>n</kbd> / <kbd>p</kbd> でハンク間を移動します。")),
            ("shot", "diff", t(
                "Added lines in green, removed in red, with lightweight syntax colouring on top.",
                "追加行は緑、削除行は赤。その上に軽量なシンタックスカラーが乗ります。")),
            ("shot", "diff-blame", t(
                "<kbd>b</kbd> adds a blame gutter — short hash and author per line. Lines you "
                "have not committed are attributed to <i>Not Committed Yet</i> rather than to "
                "whoever last touched them.",
                "<kbd>b</kbd> で blame 表示を追加します（行ごとの短縮ハッシュと作者）。"
                "未コミットの行は、直前に触った人ではなく <i>Not Committed Yet</i> と表示されます。")),
            ("keys", [
                (t("w", "w"), t("Ignore whitespace changes", "空白のみの差分を無視")),
                (t("f", "f"), t("Show the whole file, not just changed context", "変更周辺だけでなくファイル全体を表示")),
                (t("b", "b"), t("Toggle the blame gutter", "blame 表示の切り替え")),
                (t("[ / ]", "[ / ]"), t("Previous / next changed file", "前 / 次の変更ファイル")),
                (t("n / p", "n / p"), t("Next / previous hunk", "次 / 前のハンク")),
            ]),
            ("note", t(
                "Prefer your own diff tool? Set an external command in Settings with <kbd>c</kbd>. "
                "It may contain <code>{path}</code>, <code>{range}</code>, <code>{base}</code>, "
                "<code>{target}</code> and <code>{file}</code>. Programs are resolved from PATH "
                "or given as an absolute path — a relative path is rejected, since the child runs "
                "with the displayed repository as its working directory and that repository may "
                "not be yours.",
                "使い慣れた diff ツールを使いたい場合は、設定画面で <kbd>c</kbd> から外部コマンドを指定できます。"
                "<code>{path}</code> <code>{range}</code> <code>{base}</code> <code>{target}</code> "
                "<code>{file}</code> のプレースホルダが使えます。プログラムは PATH から解決するか"
                "絶対パスで指定してください。相対パスは拒否されます — 子プロセスは表示中のリポジトリを"
                "作業ディレクトリとして動くため、そのリポジトリが自分のものとは限らないからです。")),
        ],
    ),
    (
        "cross-repo",
        t("Across every repository", "リポジトリ横断"),
        [
            ("shot", "commit-search", t(
                "<kbd>S</kbd> searches commit messages in every registered repository at once — "
                "a case-insensitive substring, not a regex. <kbd>Enter</kbd> jumps straight into "
                "the matching commit's diff. A repository the search could not reach is reported "
                "as failed rather than silently counted as having no matches.",
                "<kbd>S</kbd> で登録済み全リポジトリのコミットメッセージを一括検索します"
                "（正規表現ではなく大文字小文字を無視した部分一致）。<kbd>Enter</kbd> でそのコミットの"
                "差分に直接移動します。検索できなかったリポジトリは、"
                "「一致なし」と混同されないよう失敗として報告されます。")),
            ("shot", "global-members", t(
                "<kbd>M</kbd> aggregates contributors across all repositories, merging aliases "
                "into one person. <kbd>Enter</kbd> jumps into the repository under the cursor.",
                "<kbd>M</kbd> で全リポジトリの貢献者を集計し、別名を1人に統合します。"
                "<kbd>Enter</kbd> でカーソル位置のリポジトリに移動します。")),
        ],
    ),
    (
        "worktree-workspace",
        t("Worktrees across repositories", "リポジトリを横断するWorktree"),
        [("shot", "workspace", t("Local worktrees with personal notes and favorites.", "ローカルWorktreeと本人の用途メモ・お気に入り。")),
         ("p", t(
            "Press <kbd>W</kbd> on Home for a local worktree workspace. Each path appears once, "
            "even when both a parent and its linked worktree are registered. The table shows the first "
            "registered parent, branch, dirty count, HEAD commit date and path. SSH entries are skipped and counted.",
            "Homeの <kbd>W</kbd> でローカルWorktreeを横断表示します。親リポジトリとそのWorktreeを両方登録しても"
            "同じパスは1行です。最初に登録された親、ブランチ、未コミット数、HEADのコミット日時、パスを表示し、"
            "SSHは対象外として件数を示します。")),
         ("p", t(
            "Use <kbd>/</kbd> to search paths, branches and purpose notes. <kbd>m</kbd> edits your note; "
            "<kbd>*</kbd> toggles a favorite and <kbd>f</kbd> filters favorites. Notes and favorites are "
            "stored in preferences by canonical local path. <kbd>Enter</kbd> or <kbd>O</kbd> opens the tool menu; "
            "<kbd>t</kbd> starts a shell in that worktree. <kbd>r</kbd> refreshes in the background while "
            "navigation remains available. Rows may show previous values until refreshed; use <kbd>[</kbd>/<kbd>]</kbd> to inspect repository collection errors even when healthy rows are selected. Errors stay distinct "
            "from a clean worktree. These are Git states and personal notes, not AI agent activity.",
            "<kbd>/</kbd> でパス・ブランチ・用途メモを検索し、<kbd>m</kbd> でメモを編集します。"
            "<kbd>*</kbd> でお気に入りを切り替え、<kbd>f</kbd> で絞り込みます。メモとお気に入りは正規化した"
            "ローカルパスをキーに設定へ保存します。<kbd>Enter</kbd> または <kbd>O</kbd> でツールメニュー、"
            "<kbd>t</kbd> でそのWorktreeのシェルを開けます。<kbd>r</kbd> のバックグラウンド再取得中も操作できます。"
            "取得中は前回値が残る場合があります。正常な行を選択中も <kbd>[</kbd>/<kbd>]</kbd> で各リポジトリの取得失敗を確認でき、クリーンなWorktreeと区別します。表示するのはGit状態と本人のメモであり、"
            "AIエージェントの活動状況ではありません。"))],
    ),
    (
        "work-tools",
        t("Open your work tools", "普段の作業ツールで開く"),
        [("shot", "open-tools", t("Choose a local work tool; unavailable commands are marked.", "ローカルの作業ツールを選択。未導入コマンドも明示します。")),
         ("p", t(
            "Press <kbd>O</kbd> on Home, Repo or Diff. Choose <kbd>t</kbd> for a shell, "
            "<kbd>e</kbd> for the editor, <kbd>l</kbd> for lazygit, or <kbd>g</kbd> for GitUI. "
            "Unavailable commands are marked; install clients on PATH. SSH repositories do not support local tool launch.",
            "Home・詳細・diffで <kbd>O</kbd> を押し、<kbd>t</kbd>（シェル）、<kbd>e</kbd>（エディタ）、"
            "<kbd>l</kbd>（lazygit）、<kbd>g</kbd>（GitUI）を選びます。未導入ツールは明示されるのでPATH上に導入してください。"
            "SSHリポジトリのローカルツール起動は非対応です。")),
         ("p", t(
            "Configure the editor with <kbd>c</kbd> in that menu. Quoted arguments and <code>{path}</code> "
            "are supported; without that placeholder the repository path is appended as one argument. "
            "Use <kbd>w</kbd> to enable waiting for terminal editors; GUI launch does not wait by default. "
            "After a terminal tool exits the dashboard restores input and reloads the selected repository. "
            "For subsequent GUI edits use <kbd>r</kbd> to reload. Commands must be absolute paths or names on PATH.",
            "メニューの <kbd>c</kbd> でエディタを設定できます。引数の引用符と <code>{path}</code> に対応し、"
            "プレースホルダがなければリポジトリパスを1引数として追加します。端末内エディタは <kbd>w</kbd> で終了待機をオンにします。"
            "GUIは既定で待機しません。端末内ツール終了後は表示・入力を復帰し、選択リポジトリを再取得します。"
            "GUIで後から編集した内容は <kbd>r</kbd> で再取得してください。コマンドは絶対パスかPATH上の名前を指定します。"))],
    ),
    (
        "settings",
        t("Settings and files", "設定とファイル"),
        [
            ("shot", "settings", t(
                "<kbd>s</kbd> from the dashboard. Language, theme, external diff command, and "
                "the Home auto-refresh interval.",
                "ダッシュボードで <kbd>s</kbd>。表示言語、テーマ、外部 diff コマンド、"
                "Home の自動更新間隔を設定できます。")),
            ("p", t(
                "Configuration lives in the OS config directory — "
                "<code>~/Library/Application Support/com.git-dashboard.git-dashboard/</code> on "
                "macOS, <code>~/.config/git-dashboard/</code> on Linux, "
                "<code>%APPDATA%\\git-dashboard\\git-dashboard\\config\\</code> on Windows.",
                "設定は OS の設定ディレクトリに保存されます — macOS は "
                "<code>~/Library/Application Support/com.git-dashboard.git-dashboard/</code>、"
                "Linux は <code>~/.config/git-dashboard/</code>、Windows は "
                "<code>%APPDATA%\\git-dashboard\\git-dashboard\\config\\</code>。")),
            ("cols", [
                ("config.json", t("Registered repositories: name, path, group.",
                                  "登録済みリポジトリ（名前・パス・グループ）。")),
                ("members.json", t("Team members and the commit-author aliases merged into them.",
                                   "チームメンバーと、そこへ統合するコミット作者の別名。")),
                ("prefs.json", t("Language, theme, sidebar, diff toggles, recent comparisons, sort order, refresh interval, external diff command, editor command/wait, onboarding dismissal, and worktree notes/favorites.",
                                 "表示言語・テーマ・サイドバー・差分トグル・最近の比較・並び順・更新間隔・外部diffコマンド・エディタと終了待機・初回案内の表示済み状態・Worktreeメモとお気に入り。")),
                ("tech_rules.json", t("Rules for detecting language and framework versions.",
                                      "言語・フレームワークのバージョン検出ルール。")),
                ("cache/", t("Cached dashboard rows and repository snapshots. Safe to delete; "
                             "rebuilt on the next refresh.",
                             "ダッシュボード行とリポジトリ解析結果のキャッシュ。"
                             "削除しても次回更新時に再生成されます。")),
            ]),
            ("note", t(
                "Auto-refresh is off by default. Local reload does not fetch remote Git refs. "
                "GitHub information is cached for five minutes, with a one-minute retry delay after failure; the next reload after expiry triggers the request. "
                "Home shows the selected repository's local check time, latest repository-wide CI branch, "
                "authentication/fetch state and stale cache. Press <kbd>C</kbd> to open its CI run. "
                "PR counts of 100 or more are shown as <code>100+</code>.",
                "自動更新は既定でオフです。ローカルの再読み込みではリモートのGit参照をfetchしません。"
                "GitHub情報は5分間キャッシュし、取得失敗後は1分経過後の再読込で再試行します。"
                "Homeにはローカル取得時刻、リポジトリ全体の最新CI実行のブランチ、未認証・取得失敗・"
                "古いキャッシュを表示します。<kbd>C</kbd> でCI実行ページを開けます。"
                "PRを100件取得した場合は <code>100+</code> と表示します。")),
        ],
    ),
    (
        "trust",
        t("What it does with a repository you do not trust", "信頼できないリポジトリの扱い"),
        [
            ("p", t(
                "Registering a repository means running git inside it, and git reads that "
                "repository's own configuration. Several config keys are enough to execute "
                "arbitrary commands, so every invocation neutralises them: "
                "<code>core.fsmonitor</code>, <code>core.sshCommand</code>, "
                "<code>uploadpack.packObjectsHook</code> and <code>protocol.ext.allow</code>, "
                "plus <code>--no-ext-diff --no-textconv</code> so <code>diff.external</code> and "
                "textconv drivers cannot fire. Git's own <code>safe.directory</code> does not "
                "cover this.",
                "リポジトリを登録するとその中で git を実行することになり、"
                "git はそのリポジトリ自身の設定を読みます。"
                "いくつかの設定キーは任意コマンドの実行に十分なため、毎回の実行時に無効化しています — "
                "<code>core.fsmonitor</code>、<code>core.sshCommand</code>、"
                "<code>uploadpack.packObjectsHook</code>、<code>protocol.ext.allow</code>。"
                "加えて <code>--no-ext-diff --no-textconv</code> を渡し、"
                "<code>diff.external</code> や textconv ドライバが起動しないようにしています。"
                "git の <code>safe.directory</code> はこの範囲をカバーしません。")),
            ("p", t(
                "Repository <i>content</i> is untrusted in the same way. File text, commit "
                "messages, branch names and author names all reach the screen, and a terminal "
                "executes what it is sent — so escape sequences are removed from everything git "
                "returns, at the single point where its output becomes a string. Tabs are "
                "expanded where content is displayed, so a tab-indented file cannot push text "
                "out of its pane.",
                "リポジトリの<i>内容</i>も同様に信頼していません。"
                "ファイル本文・コミットメッセージ・ブランチ名・作者名はいずれも画面に届き、"
                "端末は送られたものを実行します。そのため git の出力が文字列になる唯一の地点で"
                "エスケープシーケンスを除去しています。"
                "またファイル内容を表示する箇所ではタブを展開しており、"
                "タブでインデントされたファイルがペインの外へ文字を押し出すことはありません。")),
            ("p", t(
                "An <code>ssh://</code> locator whose host begins with <code>-</code> would be "
                "passed to <code>ssh</code> as an option, so hosts are validated against an "
                "allowlist. When several git commands share one SSH connection, the delimiter "
                "between their outputs is an unpredictable per-call nonce — a fixed one could be "
                "forged by a commit author named after it.",
                "<code>-</code> で始まるホストを持つ <code>ssh://</code> は "
                "<code>ssh</code> にオプションとして渡ってしまうため、ホストを許可リストで検証しています。"
                "複数の git コマンドが1本の SSH 接続を共有する際、出力を区切る文字列は"
                "呼び出しごとの予測不能な nonce です — 固定文字列だと、"
                "それを名前にしたコミット作者に偽装される恐れがあるためです。")),
        ],
    ),
    (
        "help",
        t("Everything else", "その他"),
        [
            ("shot", "help", t(
                "<kbd>?</kbd> from any screen lists every binding, scrollable with "
                "<kbd>j</kbd>/<kbd>k</kbd> or the wheel. The footer shows the bindings for the "
                "current screen, wrapping to a second row when they do not fit one.",
                "どの画面でも <kbd>?</kbd> で全キー操作の一覧が出ます。"
                "<kbd>j</kbd>/<kbd>k</kbd> やホイールでスクロールできます。"
                "フッターには現在の画面のキー操作が表示され、1行に収まらない場合は2行に折り返します。")),
            ("keys", [
                (t("q / Ctrl+C", "q / Ctrl+C"), t("Quit", "終了")),
                (t("Esc / h", "Esc / h"), t("Back one screen", "1つ前の画面に戻る")),
                (t("g / G", "g / G"), t("First / last item", "先頭 / 末尾へ")),
                (t("t / T", "t / T"), t("Open a shell in the repository directory", "リポジトリのディレクトリでシェルを開く")),
            ]),
        ],
    ),
]


# --- rendering -------------------------------------------------------------
SECTIONS[3:3] = task_sections(t)

CSS = """
:root{
  --bg:#ffffff; --page:#f7f8fb; --ink:#1b2430; --muted:#5a6779; --faint:#8b98a9;
  --line:#e2e7ef; --accent:#5b6ee1; --accent-soft:#eef0fd; --accent-line:#c9d0f7;
  --code-bg:#f2f4f9; --pre-bg:#1e1e2e; --pre-fg:#cdd6f4;
  --green:#2f8a4f; --red:#c0453f; --yellow:#9a6b00; --blue:#3d63c9;
  --shadow:0 6px 24px rgba(30,45,90,.07);
  --mono:ui-monospace,SFMono-Regular,Menlo,Consolas,'DejaVu Sans Mono',monospace;
  --sans:-apple-system,BlinkMacSystemFont,'Hiragino Kaku Gothic ProN','Yu Gothic',
         Meiryo,'Segoe UI',system-ui,sans-serif;
}
@media (prefers-color-scheme: dark){
  :root{
    --bg:#11131b; --page:#0d0f16; --ink:#dfe5ee; --muted:#95a1b3; --faint:#67748a;
    --line:#232839; --accent:#8b9cf5; --accent-soft:#1b1f33; --accent-line:#2f3654;
    --code-bg:#191d2a; --pre-bg:#181825; --pre-fg:#cdd6f4;
    --green:#7fd39a; --red:#f0888a; --yellow:#e6c07b; --blue:#8ab0f0;
    --shadow:0 6px 24px rgba(0,0,0,.4);
  }
}
*{box-sizing:border-box}
body{margin:0;background:var(--page);color:var(--ink);font-family:var(--sans);
     line-height:1.75;-webkit-font-smoothing:antialiased}
.wrap{max-width:1120px;margin:0 auto;padding:0 24px 96px}
header.top{background:var(--bg);border-bottom:1px solid var(--line);padding:40px 0 30px;
           margin-bottom:34px}
header.top .wrap{padding-bottom:0}
h1{margin:0 0 6px;font-size:2rem;letter-spacing:-.02em}
h1 .dim{color:var(--faint);font-weight:400}
.tagline{margin:0;color:var(--muted);font-size:1.05rem}
.langbar{margin-top:18px;display:flex;gap:8px;flex-wrap:wrap;align-items:center}
.langbar a{font-size:.85rem;padding:5px 12px;border:1px solid var(--line);border-radius:999px;
           color:var(--muted);text-decoration:none;background:var(--bg)}
.langbar a:hover{border-color:var(--accent-line);color:var(--accent)}
.langbar a.on{background:var(--accent-soft);border-color:var(--accent-line);color:var(--accent);
              font-weight:600}
nav.toc{background:var(--bg);border:1px solid var(--line);border-radius:12px;padding:16px 20px;
        margin-bottom:38px;box-shadow:var(--shadow)}
nav.toc ol{margin:0;padding-left:20px;columns:2;column-gap:34px}
nav.toc li{margin:3px 0}
nav.toc a{color:var(--muted);text-decoration:none}
nav.toc a:hover{color:var(--accent)}
section{background:var(--bg);border:1px solid var(--line);border-radius:14px;
        padding:26px 30px 30px;margin-bottom:26px;box-shadow:var(--shadow)}
h2{margin:0 0 14px;font-size:1.4rem;letter-spacing:-.01em;scroll-margin-top:20px}
h2 a{color:var(--faint);text-decoration:none;font-weight:400;font-size:.8em;margin-left:8px;
     opacity:0;transition:opacity .15s}
h2:hover a{opacity:1}
p{margin:0 0 14px}
figure{margin:22px 0 8px}
figure img{width:100%;height:auto;display:block;border-radius:10px;
           border:1px solid var(--line);background:var(--pre-bg)}
figcaption{margin-top:10px;color:var(--muted);font-size:.9rem;line-height:1.65}
table{width:100%;border-collapse:collapse;margin:8px 0 16px;font-size:.93rem}
th,td{text-align:left;padding:9px 12px;border-bottom:1px solid var(--line);vertical-align:top}
th{color:var(--faint);font-weight:600;font-size:.8rem;text-transform:uppercase;
   letter-spacing:.04em}
tbody tr:last-child td{border-bottom:none}
td.k{white-space:nowrap;width:1%}
td.term{width:26%;font-weight:600}
th,td{overflow-wrap:anywhere}
code,kbd{font-family:var(--mono);font-size:.88em}
code{background:var(--code-bg);padding:1px 5px;border-radius:4px}
kbd{background:var(--code-bg);border:1px solid var(--line);border-bottom-width:2px;
    padding:1px 6px;border-radius:5px;white-space:nowrap}
pre{background:var(--pre-bg);color:var(--pre-fg);padding:16px 18px;border-radius:10px;
    overflow-x:auto;font-family:var(--mono);font-size:.88rem;line-height:1.6}
pre code{background:none;padding:0}
.aside{border-left:3px solid var(--accent-line);background:var(--accent-soft);
       padding:12px 16px;border-radius:0 8px 8px 0;margin:16px 0;font-size:.94rem}
.aside.warn{border-left-color:var(--red);background:color-mix(in srgb,var(--red) 8%,transparent)}
b.red{color:var(--red)} b.green{color:var(--green)}
b.yellow{color:var(--yellow)} b.blue{color:var(--blue)}
footer{color:var(--faint);font-size:.85rem;text-align:center;padding-top:10px}
footer a{color:var(--muted)}
@media (max-width:720px){
  nav.toc ol{columns:1}
  section{padding:20px 18px 24px}
  .wrap{padding:0 14px 60px}
}
"""

STRINGS = {
    "title": t("git-dashboard-tui — User Manual", "git-dashboard-tui — ユーザーマニュアル"),
    "tagline": t(
        "Find your next task across repositories, from one terminal.",
        "散らばったリポジトリから、次に取りかかる場所が見える。"),
    "desc": t(
        "Install, run and drive git-dashboard-tui: the dashboard, the repository tabs, "
        "the diff viewer, cross-repository search, settings and keybindings.",
        "git-dashboard-tui の導入と操作 — ダッシュボード、リポジトリ各タブ、差分ビューア、"
        "横断検索、設定、キー操作の解説。"),
    "contents": t("Contents", "目次"),
    "repo": t("Repository", "リポジトリ"),
    "readme": t("README", "README"),
    "changelog": t("Changelog", "変更履歴"),
    "other_lang": t("日本語", "English"),
    "key": t("Key", "キー"),
    "action": t("Action", "動作"),
    "item": t("Item", "項目"),
    "meaning": t("Meaning", "意味"),
    "generated": t(
        "Screenshots are captured from the running program by "
        "<code>docs/gen_shots.py</code> against the demo workspace built by "
        "<code>docs/gen_demo.py</code>. They record the captured build, not a live view; timestamps and environment can change on regeneration.",
        "スクリーンショットは <code>docs/gen_demo.py</code> が生成するデモ環境に対して "
        "<code>docs/gen_shots.py</code> が実行中のプログラムから取得しています。"
        "撮影したビルドの表示を記録したもので、ライブ表示ではありません。再生成時は取得時刻や環境によって変わる場合があります。"),
}


def render(lang: int) -> str:
    L = lang
    esc = html.escape
    out: list[str] = []
    other = "manual.ja.html" if L == EN else "manual.html"
    lang_code = "en" if L == EN else "ja"

    out.append(f'<!doctype html>\n<html lang="{lang_code}">\n<head>')
    out.append('<meta charset="utf-8">')
    out.append('<meta name="viewport" content="width=device-width, initial-scale=1">')
    out.append(f"<title>{esc(STRINGS['title'][L])}</title>")
    out.append(f'<meta name="description" content="{esc(STRINGS["desc"][L])}">')
    out.append(f'<meta property="og:title" content="{esc(STRINGS["title"][L])}">')
    out.append(f'<meta property="og:description" content="{esc(STRINGS["desc"][L])}">')
    out.append('<meta property="og:type" content="website">')
    out.append('<meta property="og:image" content="'
               'https://orapli.github.io/git-dashboard-tui/img/home.svg">')
    out.append(f"<style>{CSS}</style>")
    out.append("</head>\n<body>")

    out.append('<header class="top"><div class="wrap">')
    out.append(f'<h1>git-dashboard-tui <span class="dim">— '
               f'{esc(STRINGS["title"][L].split("— ")[1])}</span></h1>')
    out.append(f'<p class="tagline">{STRINGS["tagline"][L]}</p>')
    out.append('<div class="langbar">')
    out.append(f'<a class="on" href="#">{"English" if L == EN else "日本語"}</a>')
    out.append(f'<a href="{other}">{STRINGS["other_lang"][L]}</a>')
    out.append('<a href="https://github.com/orapli/git-dashboard-tui">'
               f'{esc(STRINGS["repo"][L])}</a>')
    out.append('<a href="https://github.com/orapli/git-dashboard-tui/blob/main/'
               f'{"README.md" if L == EN else "README.ja.md"}">{esc(STRINGS["readme"][L])}</a>')
    out.append('<a href="https://github.com/orapli/git-dashboard-tui/blob/main/CHANGELOG.md">'
               f'{esc(STRINGS["changelog"][L])}</a>')
    out.append("</div></div></header>")

    out.append('<div class="wrap">')
    out.append(f'<nav class="toc"><strong>{esc(STRINGS["contents"][L])}</strong><ol>')
    for sid, title, _ in SECTIONS:
        out.append(f'<li><a href="#{sid}">{esc(title[L])}</a></li>')
    out.append("</ol></nav>")

    suffix = "" if L == EN else ".ja"
    for sid, title, blocks in SECTIONS:
        out.append(f'<section id="{sid}">')
        out.append(f'<h2>{esc(title[L])}<a href="#{sid}" aria-label="link">#</a></h2>')
        for block in blocks:
            kind = block[0]
            if kind == "p":
                out.append(f"<p>{block[1][L]}</p>")
            elif kind == "code":
                out.append(f"<pre><code>{esc(block[1][L])}</code></pre>")
            elif kind == "note":
                out.append(f'<div class="aside">{block[1][L]}</div>')
            elif kind == "warn":
                out.append(f'<div class="aside warn">{block[1][L]}</div>')
            elif kind == "shot":
                stem, cap = block[1], block[2]
                out.append(
                    f'<figure><img src="img/{stem}{suffix}.svg" loading="lazy" '
                    f'alt="{esc(cap[L][:120])}">'
                    f"<figcaption>{cap[L]}</figcaption></figure>"
                )
            elif kind in ("keys", "cols"):
                h1 = STRINGS["key" if kind == "keys" else "item"][L]
                h2 = STRINGS["action" if kind == "keys" else "meaning"][L]
                out.append(f"<table><thead><tr><th>{esc(h1)}</th>"
                           f"<th>{esc(h2)}</th></tr></thead><tbody>")
                for name, desc in block[1]:
                    label = name[L] if isinstance(name, tuple) else name
                    tag = "kbd" if kind == "keys" else "span"
                    cell_class = "k" if kind == "keys" else "term"
                    out.append(f'<tr><td class="{cell_class}"><{tag}>{esc(label)}</{tag}></td>'
                               f"<td>{desc[L]}</td></tr>")
                out.append("</tbody></table>")
        out.append("</section>")

    out.append(f'<footer><p>{STRINGS["generated"][L]}</p></footer>')
    out.append("</div></body></html>")
    return "\n".join(out)


INDEX_CSS = """
.hero{padding:56px 0 34px;text-align:center}
.hero h1{font-size:2.6rem;margin-bottom:10px}
.hero p.tagline{font-size:1.15rem;max-width:640px;margin:0 auto 26px}
.cta{display:flex;gap:10px;justify-content:center;flex-wrap:wrap;margin-bottom:12px}
.cta a{padding:10px 20px;border-radius:9px;text-decoration:none;font-weight:600;
       border:1px solid var(--line);color:var(--ink);background:var(--bg)}
.cta a.primary{background:var(--accent);border-color:var(--accent);color:#fff}
.cta a:hover{border-color:var(--accent-line)}
.shotwrap{margin:34px auto 0;max-width:1000px}
.shotwrap img{width:100%;border-radius:12px;border:1px solid var(--line);box-shadow:var(--shadow)}
.grid{display:grid;grid-template-columns:repeat(auto-fit,minmax(260px,1fr));gap:16px;margin-top:8px}
.grid .card{background:var(--bg);border:1px solid var(--line);border-radius:12px;padding:18px 20px;
            box-shadow:var(--shadow)}
.grid .card h3{margin:0 0 6px;font-size:1.03rem}
.grid .card p{margin:0;color:var(--muted);font-size:.93rem}
"""

INDEX = {
    "points": [
        (t("See every repository at once",
           "全リポジトリを一度に把握"),
         t("Branch, ahead/behind, uncommitted changes, open PRs and CI, for as many "
           "checkouts as you have registered.",
           "ブランチ、先行/遅れ、未コミット、オープン PR と CI を、"
           "登録したチェックアウトの数だけ一覧できます。")),
        (t("Read-only about your history",
           "履歴には触らない"),
         t("It fetches and pulls. It never stages, commits, branches, checks out or "
           "pushes — so it is safe to leave open.",
           "fetch と pull は行いますが、stage・commit・branch・checkout・push は行いません。"
           "開きっぱなしにしても安全です。")),
        (t("Careful with repositories you did not write",
           "自分が書いていないリポジトリにも慎重"),
         t("Config keys that can execute commands are neutralised on every git call, and "
           "escape sequences are stripped from repository content before it reaches your "
           "terminal.",
           "コマンド実行につながる設定キーを毎回無効化し、"
           "リポジトリの内容が端末に届く前にエスケープシーケンスを除去します。")),
        (t("Bilingual, themed, mouse-aware",
           "日英対応・テーマ・マウス操作"),
         t("English and Japanese throughout, Catppuccin Mocha and Latte, and click to "
           "sort, select and compare.",
           "全体が日英対応。Catppuccin Mocha / Latte のテーマ、"
           "クリックでの並び替え・選択・比較に対応。")),
    ],
}


def render_index() -> str:
    esc = html.escape
    out = ['<!doctype html>', '<html lang="en">', "<head>",
           '<meta charset="utf-8">',
           '<meta name="viewport" content="width=device-width, initial-scale=1">',
           "<title>git-dashboard-tui</title>",
           f'<meta name="description" content="{esc(STRINGS["desc"][EN])}">',
           '<meta property="og:title" content="git-dashboard-tui">',
           f'<meta property="og:description" content="{esc(STRINGS["desc"][EN])}">',
           '<meta property="og:image" content="'
           'https://orapli.github.io/git-dashboard-tui/img/home.svg">',
           f"<style>{CSS}{INDEX_CSS}</style>", "</head>", "<body>",
           '<div class="wrap">', '<div class="hero">',
           "<h1>git-dashboard-tui</h1>",
           f'<p class="tagline">{STRINGS["tagline"][EN]}<br>'
           f'<span style="color:var(--faint)">{STRINGS["tagline"][JA]}</span></p>',
           '<div class="cta">',
           '<a class="primary" href="manual.html">User Manual</a>',
           '<a href="manual.ja.html">日本語マニュアル</a>',
           '<a href="https://github.com/orapli/git-dashboard-tui">GitHub</a>',
           "</div>",
           '<div class="shotwrap"><img src="img/home.svg" '
           'alt="The dashboard, listing five repositories with their branch, sync state, '
           'uncommitted changes and last commit"></div>',
           "</div>",
           "<section><h2>Install</h2>",
           "<pre><code>curl -fsSL https://raw.githubusercontent.com/orapli/"
           "git-dashboard-tui/main/install.sh | sh</code></pre>",
           '<p style="color:var(--muted);font-size:.93rem">Linux (x86_64, ARM64), macOS '
           "(Apple Silicon, Intel) and Windows (x64). Verifies the published SHA-256 and "
           "never uses sudo. The shell installer supports Linux/macOS; Windows uses the release ZIP.</p><p>This site documents development main; the installer downloads the latest release. / このサイトはmainの開発版、インストーラは最新リリースを対象とします。 <a href='manual.html#versions'>Version details</a> · <a href='manual.ja.html#versions'>バージョンの説明</a></p></section>",
           "<section><h2>What it is</h2>", '<div class="grid">']
    for title, body in INDEX["points"]:
        out.append(f'<div class="card"><h3>{esc(title[EN])}</h3><p>{body[EN]}</p></div>')
    out.append("</div></section>")
    out.append(f'<footer><p>{STRINGS["generated"][EN]}</p></footer>')
    out.append("</div></body></html>")
    return "\n".join(out)

def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--out", default="docs")
    args = ap.parse_args()
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    for lang, name in ((EN, "manual.html"), (JA, "manual.ja.html")):
        path = out / name
        path.write_text(render(lang), encoding="utf-8")
        print(f"  {path}  ({len(path.read_text(encoding='utf-8')) // 1024} KB)")
    index = out / "index.html"
    index.write_text(render_index(), encoding="utf-8")
    print(f"  {index}  ({len(index.read_text(encoding='utf-8')) // 1024} KB)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
