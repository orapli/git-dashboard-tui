# Documentation sources and maintenance

The READMEs introduce installation and the first workflow. Detailed keys and config
live in `reference.md` / `reference.ja.md`. The illustrated manual is generated from
bilingual Python content; edit its sources, then regenerate the HTML.

| File | Purpose |
|---|---|
| `gen_manual.py` | Shared rendering, screen reference and landing page |
| `manual_tasks.py` | Paired English/Japanese tasks, release scope, status semantics, editor examples, support matrix and troubleshooting |
| `reference.md` / `reference.ja.md` | Hand-maintained keyboard and configuration tables |
| `gen_demo.py` | Synthetic Git repositories with fixed commit timestamps and invented authors |
| `gen_shots.py` | Drives the real binary in a PTY and writes terminal screenshots as SVG |
| `gen_tour.py` | Captures a 28-second registration → attention → diff → shell tour |
| `check_docs.py` | Checks generated HTML, local links/anchors, image assets, language structure and MSRV consistency |
| `img/` | Captured SVGs; `<stem>.svg` is English and `<stem>.ja.svg` is Japanese |

## Text-only changes

Use Python 3.11 or newer. Generation and validation use only the standard library.
Run from the repository root:

```bash
python3 docs/gen_manual.py
python3 docs/check_docs.py
python3 -m unittest discover -s docs -p 'test_check_docs.py'
```

The checker resolves repository-relative links plus this project's GitHub Pages,
GitHub `blob/main` and raw `main` URLs against the checkout, including fragments.
It checks SVG XML, paired image names, HTML alt text, generated-file freshness, and
README/reference heading, table, code-block and link structure across languages.
It does not check external websites' availability or translation meaning. Review
wording in both languages and open external links changed by your edit.
Generated OpenWiki content is not traversed or edited; links into it must exist.

CI runs these checks on main pushes and PRs. The independent MSRV job checks the
locked dependency graph and all Rust targets with the version declared in
`Cargo.toml`. When raising MSRV, update its CI job, READMEs, manual and developer guide.

## Recapture after visible UI changes

Build the binary matching the source you are documenting. PTY capture needs a Unix
host and the third-party `pyte` terminal emulator; use an isolated environment:

```bash
cargo build --release --locked
python3 -m venv /tmp/gdt-docs-venv
/tmp/gdt-docs-venv/bin/pip install pyte
/tmp/gdt-docs-venv/bin/python docs/gen_demo.py /tmp/gdt-demo
/tmp/gdt-docs-venv/bin/python docs/gen_shots.py --bin target/release/git-dashboard-tui --workspace /tmp/gdt-demo --lang en
/tmp/gdt-docs-venv/bin/python docs/gen_shots.py --bin target/release/git-dashboard-tui --workspace /tmp/gdt-demo --lang ja
/tmp/gdt-docs-venv/bin/python docs/gen_tour.py --bin target/release/git-dashboard-tui --workspace /tmp/gdt-demo --lang en
/tmp/gdt-docs-venv/bin/python docs/gen_tour.py --bin target/release/git-dashboard-tui --workspace /tmp/gdt-demo --lang ja
python3 docs/gen_manual.py
python3 docs/check_docs.py
```

For a custom `CARGO_TARGET_DIR`, pass that build's absolute binary path to `--bin`.
Use `gen_shots.py --only <scene>` to recapture one scene in each language.
Captures set an isolated `GIT_DASHBOARD_CONFIG_DIR` and do not use your real TUI
configuration. The tour's final tool is a plain shell.

Fixed demo commit timestamps do **not** guarantee byte-identical screenshots.
Home records actual local/GitHub check times; terminal size, fonts, build version
and asynchronous completion can also affect output. A screenshot records the
captured build and must be recaptured after relevant UI changes. Do not regenerate
all images for a prose-only edit just to change their timestamps.

## Visual review and publishing

1. Serve `docs/` locally with `python3 -m http.server 8000 --directory docs`.
2. Open `http://localhost:8000/`, `http://localhost:8000/manual.html` and
   `http://localhost:8000/manual.ja.html`. Check navigation, tables, screenshots and
   code blocks at desktop and narrow mobile widths.
3. When changing tour assets, check the animation and the `prefers-reduced-motion`
   still image. Record the binary commit, OS and verification in the change description.
4. Commit generated HTML with its source changes. The site and main README describe
   development main; the installer fetches a release binary. Keep unreleased features
   under `Unreleased` in `CHANGELOG.md` and retain a visible version note on the site.
5. At release time, move shipped changelog entries to the new version, adjust the
   “after v0.3.1” feature notes to the actual shipping release, and confirm the published
   site matches the intended branch in GitHub Pages settings. A documentation commit
   alone does not create a release binary.

OpenWiki is separate generated architecture context. Its workflow is manual-only;
check `openwiki/.last-update.json` for the source commit and refresh via its workflow
when needed. Do not hand-edit generated wiki pages.
