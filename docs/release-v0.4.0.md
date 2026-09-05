# v0.4.0 — Multi-repository workspaces

Find repositories needing attention, inspect changes, and open your usual tools from one terminal.

- Import work folders with cancellable background discovery and a first-run guide.
- Read Home attention reasons, GitHub freshness and the branch of the latest CI run.
- Open a shell, editor, lazygit or GitUI with `O`; resume a local worktree with `W`.
- Search worktrees, save purpose notes and favorites, and distinguish short directory names.
- Inspect untracked files, including work before the first commit. Status and Diff now agree.
- Refresh changed-file lists after leaving external tools, including symlink registrations.
- Keep GitHub caches separate when remotes change; inspect partial Worktree failures.
- Use the updated English/Japanese manuals and full-color walkthroughs.

Pre-built binaries cover Linux x86_64/ARM64, macOS Intel/Apple Silicon and Windows x64.
Verify archives with the included `SHA256SUMS`, or use the installer on Linux/macOS:

```bash
curl -fsSL https://raw.githubusercontent.com/orapli/git-dashboard-tui/main/install.sh | sh
```

Building from source requires **Rust 1.88 or later**. Existing configuration remains usable;
back up your configuration before upgrading. No explicit migration command is required.

CI checks Linux x86_64/ARM64, macOS and Windows, plus documentation and MSRV.
GitHub CI information is the repository-wide latest run, not an aggregate of all workflows.
SSH registrations do not support local tool launch, GitHub status or the cross-repository workspace.

# 日本語

複数リポジトリの状態確認から普段の作業ツールへの移動までを整えたリリースです。

- フォルダ走査をバックグラウンド化し、中止・順次表示・初回ガイドに対応。
- Homeに要対応の理由、GitHub情報の鮮度、最新CI実行のブランチを表示。
- `O`でシェル・エディタ・lazygit・GitUI、`W`で横断Worktree一覧を開く。
- Worktreeの検索、用途メモ、お気に入り、短いディレクトリ名表示に対応。
- 未追跡ファイルと初回コミット前の差分を表示し、件数と一覧の不一致を修正。
- 外部ツール終了後の一覧更新、別名パス、リモート変更時のキャッシュ、取得失敗表示を修正。
- 日英マニュアルとカラーの操作デモを更新。

ソースからのビルドには **Rust 1.88以上** が必要です。既存設定を継続利用でき、専用の移行操作は不要です。
配布対象はLinux x86_64/ARM64、macOS Intel/Apple Silicon、Windows x64です。
GitHub CIはリポジトリ全体の最新1実行で、全ワークフローの集約ではありません。
SSH登録のローカルツール起動・GitHub情報・横断Worktree一覧は非対応です。
