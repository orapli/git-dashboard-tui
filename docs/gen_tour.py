#!/usr/bin/env python3
"""Capture a 28-second, four-step walkthrough from the real TUI (requires pyte).

python3 docs/gen_demo.py /tmp/gdt-tour-workspace
python3 docs/gen_tour.py --workspace /tmp/gdt-tour-workspace --lang ja
"""
import argparse
import json
import tempfile
import xml.etree.ElementTree as ET
from pathlib import Path

from gen_shots import Session


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--bin", default="target/debug/git-dashboard-tui")
    ap.add_argument("--workspace", required=True)
    ap.add_argument("--lang", choices=["en", "ja"], default="en")
    ap.add_argument("--out", default="docs/img")
    args = ap.parse_args()
    ja = args.lang == "ja"
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    frames = []
    with tempfile.TemporaryDirectory(prefix="gdt-tour-") as tmp:
        home = Path(tmp)
        cfg = home / ".config/git-dashboard"
        cfg.mkdir(parents=True)
        (cfg / "config.json").write_text("[]")
        (cfg / "prefs.json").write_text(json.dumps({"language": "Japanese" if ja else "English"}))
        session = Session(Path(args.bin).resolve(), home, 110, 24, {"SHELL": "/bin/sh", "ENV": "/dev/null"})

        def capture(title, required):
            screen = "\n".join(session.screen.display)
            if required not in screen:
                raise RuntimeError(f"Missing {required!r} in captured screen:\n{screen}")
            if "\ufffd" in screen:
                raise RuntimeError(f"Invalid UTF-8 in captured screen:\n{screen}")
            frames.append(session.to_svg(title))

        try:
            session.pump(1)
            capture("1/4  A: フォルダから一括登録" if ja else "1/4  A: Import a work folder", "A  ")
            session.send("A")
            session.send(str(Path(args.workspace).resolve()) + "\r", 3)
            session.send("\r", 4)
            screen = "\n".join(session.screen.display)
            if "web-frontend" not in screen:
                raise RuntimeError(f"Import failed:\n{screen}")
            frames.clear()
            capture("1/4  A: フォルダから一括登録" if ja else "1/4  A: Import a work folder", "まずはこの4つから" if ja else "Start here")
            session.send("\x1b")
            session.send("n")
            capture("2/4  n: 要対応を見つける" if ja else "2/4  n: Find what needs attention", "MERGE")
            session.send("\r", 3)
            session.send("1", 1)
            session.send("\r", 2)
            capture("3/4  Enter: 差分を確認" if ja else "3/4  Enter: Inspect the diff", "checkout.ts")
            session.send("t", 1)
            session.send("clear\r", 0.5)
            session.send("pwd\r", 0.5)
            capture("4/4  t: シェルで作業 / exit で戻る" if ja else "4/4  t: Work in a shell / exit to return", "web-frontend")
            session.send("exit\r", 1)
            screen = "\n".join(session.screen.display)
            if "checkout.ts" not in screen:
                raise RuntimeError(f"TUI did not return after shell exit:\n{screen}")
        finally:
            session.close()
    root = ET.fromstring(frames[0])
    attrs = " ".join(f'{k}="{root.attrib[k]}"' for k in ("width", "height", "viewBox"))
    svg = [f'<svg xmlns="http://www.w3.org/2000/svg" {attrs} role="img">',
           '<title>git-dashboard-tui: import, focus, inspect, work</title>',
           '<style>.step{visibility:hidden;animation:tour 28s step-end infinite}'
           '@keyframes tour{0%,24.99%{visibility:visible}25%,100%{visibility:hidden}}'
           '@media(prefers-reduced-motion:reduce){.step{animation:none}.step:first-of-type{visibility:visible}}</style>']
    for i, frame in enumerate(frames):
        svg.append(f'<g class="step" style="animation-delay:-{28-i*7}s">{frame}</g>')
    svg.append('</svg>')
    suffix = ".ja" if ja else ""
    target = out / f"quick-tour{suffix}.svg"
    target.write_text("\n".join(svg))
    print(target)


if __name__ == "__main__":
    main()
