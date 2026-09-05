# ドキュメント改善の対応記録

2026-09-05。前回のレビュー後に提案したドキュメント改善を対象とします。
アプリの既知の不具合は修正せず、現状の制限と回避手順を資料に記載しました。

| 改善案 | 対応先 |
|---|---|
| READMEを導入中心に短縮し、詳細を移動 | [日本語README](../README.ja.md)、[英語README](../README.md)、[操作・設定リファレンス](reference.ja.md) |
| 目的別の操作手順 | [マニュアル：目的別の操作手順](manual.ja.html#tasks) |
| 要対応と未確認、CIの対象、100+、キャッシュと再試行、再読込とfetchの違い | [マニュアル：状態と更新](manual.ja.html#freshness) |
| 症状→確認コマンド→対処のトラブルシューティング | [マニュアル：トラブルシューティング](manual.ja.html#troubleshooting) |
| エディタ設定例とOS・SSH対応範囲 | [エディタ例](manual.ja.html#editor-examples)、[対応表](manual.ja.html#platforms) |
| 最新リリースとmainの資料・新機能を区別 | README、マニュアル、トップページ、[Unreleased変更履歴](../CHANGELOG.md#unreleased) |
| SSHではシェルだけ非対応という誤記とprefsの項目不足を修正 | [操作・設定リファレンス](reference.ja.md)、マニュアル |
| 開発者向けモジュール構成の修正 | [CLAUDE.md](../CLAUDE.md) のapp/・git/、資料一覧と検証コマンド |
| スクリーンショットが常に同一になるという説明を修正 | [生成・保守手順](README.md)、マニュアルのフッター |
| ドキュメントCI | [検査スクリプト](check_docs.py)、[検査の回帰テスト](test_check_docs.py)、[CI](../.github/workflows/ci.yml) |
| MSRVの検証 | Rust 1.88を[Cargo.toml](../Cargo.toml)に宣言し、固定依存関係・全ターゲットを専用CIジョブでチェック |

新しいマニュアル本文は [manual_tasks.py](manual_tasks.py) に日英を並べて保持し、
[gen_manual.py](gen_manual.py) からHTMLを生成します。生成済みOpenWikiは編集していません。

## 検証

- Rust 1.88.0で `cargo check --locked --all-targets` 成功。旧記載の1.85ではなく、
  現在の言語機能と固定依存関係が要求する1.88を最低バージョンとしました。
- ドキュメント検査：生成物の一致、ローカルリンクとアンカー、SVGのXMLと日英画像ペア、
  日英の構成、MSRV記載の整合を確認。
- 検査スクリプトの回帰テスト5件：正常系、古いHTML、壊れた公開URLのアンカー、
  日本語画像の欠落、片方の言語だけの見出し追加。
- Chromiumでトップページ・日英マニュアルを1280pxと390px幅で表示。
  画像の読み込み、目次リンク、ページ全体の横はみ出し、ブラウザエラーを確認。
  追加した長文の表がモバイルで横にはみ出す問題を修正し、再確認済み。

CIのドキュメント検査は外部サイトの稼働状況や翻訳の意味的一致を保証しません。
アプリのRust実装を変更していないため、既存の238件のアプリテストとTUI撮影は再実行していません。
新しいCIジョブのGitHub上での実行と、GitHub Pagesへの反映はpush後の確認事項です。
