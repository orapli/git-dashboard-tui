# Documentation sources

The manual and every screenshot in it are generated, not hand-maintained.

| File | What it does |
|---|---|
| `gen_demo.py` | Builds the synthetic workspace the screenshots are taken against. Fixed timestamps and invented authors, so a regenerated workspace produces identical images rather than a diff on every date. |
| `gen_shots.py` | Drives the real binary in a pty and writes each screen as colour-accurate SVG. |
| `gen_tour.py` | Captures registration, attention filtering, diff inspection, and shell launch as a 28-second animated SVG. |
| `gen_manual.py` | Renders `manual.html`, `manual.ja.html` and `index.html` from one content structure. |
| `img/` | Generated screenshots. `<stem>.svg` is English, `<stem>.ja.svg` Japanese. |

Screenshots are captured rather than drawn so they cannot drift from what the
program actually renders, and both languages come out of one structure so a
section cannot gain a paragraph in English while the Japanese keeps the old text
— which the README pair has done twice.

## Regenerating

```bash
cargo build --release
pip install pyte                      # the terminal emulator the capture runs against
python3 docs/gen_demo.py  /tmp/gdt-demo
python3 docs/gen_shots.py --workspace /tmp/gdt-demo --lang en
python3 docs/gen_shots.py --workspace /tmp/gdt-demo --lang ja
python3 docs/gen_manual.py
```

`gen_shots.py --only <scene>` re-captures a single screen while iterating.

Captures use a temporary `GIT_DASHBOARD_CONFIG_DIR`, so they never read or write your
real configuration on Linux or macOS. The tour uses a plain shell for its final step.

```bash
python3 docs/gen_tour.py --bin target/release/git-dashboard-tui --workspace /tmp/gdt-demo --lang en
python3 docs/gen_tour.py --bin target/release/git-dashboard-tui --workspace /tmp/gdt-demo --lang ja
```
