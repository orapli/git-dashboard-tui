#!/usr/bin/env python3
"""Capture the running TUI as colour-accurate SVG for the manual.

Why SVG rather than PNG: the screenshots stay crisp at any zoom, they diff as
text in review, and the whole set is a few hundred KB rather than a few MB.
Why capture rather than hand-draw: a hand-drawn mock drifts from the program
the first time a colour or a column changes, and nobody notices.

The app is driven in a real pty, so what lands in the file is what the
terminal was actually sent — including every colour, which is the part a
text-only manual cannot convey.

    python3 docs/gen_shots.py [--bin PATH] [--out docs/img] [--lang en|ja]

Requires `pyte` (pip install pyte). Build the binary first.
"""

from __future__ import annotations

import argparse
import codecs
import fcntl
import json
import os
import pty
import select
import shutil
import struct
import subprocess
import sys
import termios
import time
import unicodedata
from pathlib import Path
from wcwidth import wcswidth

try:
    import pyte
except ImportError:  # pragma: no cover - a setup problem, not a code path
    raise SystemExit("pyte is required: pip install pyte")

# Catppuccin Mocha's base, used wherever a cell reports the terminal default.
# The app paints its own background, so this only covers cells it never
# touched, which must not come out transparent.
DEFAULT_BG = "1e1e2e"
DEFAULT_FG = "cdd6f4"

# Keep the terminal cell grid stable across browser font fallbacks.
# SVG textLength below also constrains box-drawing characters to their cells.
FONT_SIZE = 15.0
CELL_W = FONT_SIZE * 0.6
CELL_H = FONT_SIZE * 1.32
PAD = 14.0
CHROME_H = 26.0

FONT_STACK = (
    "ui-monospace,SFMono-Regular,Menlo,Consolas,'DejaVu Sans Mono',"
    "'Noto Sans Mono CJK JP','Hiragino Kaku Gothic ProN','Yu Gothic',monospace"
)


def char_width(ch: str) -> int:
    if not ch:
        return 0
    return 2 if unicodedata.east_asian_width(ch) in ("W", "F") else 1


def xml_escape(s: str) -> str:
    return (
        s.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")
    )


def resolve(colour: str, default: str) -> str:
    """pyte reports either a hex string, a colour name, or 'default'."""
    named = {
        "black": "45475a", "red": "f38ba8", "green": "a6e3a1",
        "brown": "f9e2af", "yellow": "f9e2af", "blue": "89b4fa",
        "magenta": "cba6f7", "cyan": "94e2d5", "white": "cdd6f4",
    }
    if colour == "default":
        return default
    if colour in named:
        return named[colour]
    return colour


class Session:
    """A live TUI in a pty, driven key by key."""

    def __init__(self, binary: Path, home: Path, cols: int, rows: int, extra_env=None):
        self.cols, self.rows = cols, rows
        self.screen = pyte.Screen(cols, rows)
        self.stream = pyte.Stream(self.screen)
        self.decoder = codecs.getincrementaldecoder("utf-8")("replace")
        env = dict(
            os.environ,
            GIT_DASHBOARD_CONFIG_DIR=str(home / ".config" / "git-dashboard"),
            TERM="xterm-256color",
            COLORTERM="truecolor",
            COLUMNS=str(cols),
            LINES=str(rows),
        )
        env.update(extra_env or {})
        self.pid, self.fd = pty.fork()
        if self.pid == 0:
            os.execve(str(binary), [str(binary)], env)
            os._exit(1)
        # ratatui draws nothing into a zero-sized pty.
        fcntl.ioctl(
            self.fd, termios.TIOCSWINSZ, struct.pack("HHHH", rows, cols, 0, 0)
        )

    def pump(self, seconds: float) -> None:
        end = time.monotonic() + seconds
        while time.monotonic() < end:
            r, _, _ = select.select([self.fd], [], [], 0.02)
            if not r:
                continue
            try:
                data = os.read(self.fd, 65536)
            except OSError:
                return
            if not data:
                return
            self.stream.feed(self.decoder.decode(data))

    def send(self, keys: str, wait: float = 0.7) -> None:
        os.write(self.fd, keys.encode())
        self.pump(wait)

    def click(self, x: int, y: int, wait: float = 0.7) -> None:
        """SGR mouse press+release at zero-based screen coordinates."""
        os.write(self.fd, f"\x1b[<0;{x + 1};{y + 1}M".encode())
        os.write(self.fd, f"\x1b[<0;{x + 1};{y + 1}m".encode())
        self.pump(wait)

    def close(self) -> None:
        try:
            os.write(self.fd, b"q")
            time.sleep(0.2)
        except OSError:
            pass
        try:
            os.kill(self.pid, 9)
        except OSError:
            pass
        try:
            # macOS can defer reaping a pty child; never hang the generator
            # after all frames have already been captured.
            deadline = time.monotonic() + 2
            while time.monotonic() < deadline:
                if os.waitpid(self.pid, os.WNOHANG)[0]:
                    break
                time.sleep(0.05)
        except OSError:
            pass
        finally:
            os.close(self.fd)

    # -- rendering ------------------------------------------------------
    def to_svg(self, title: str) -> str:
        w = PAD * 2 + CELL_W * self.cols
        h = PAD * 2 + CHROME_H + CELL_H * self.rows
        out: list[str] = [
            f'<svg xmlns="http://www.w3.org/2000/svg" width="{w:.0f}" '
            f'height="{h:.0f}" viewBox="0 0 {w:.2f} {h:.2f}" '
            f'font-family="{FONT_STACK}" font-size="{FONT_SIZE}">',
            f'<title>{xml_escape(title)}</title>',
            f'<rect width="{w:.2f}" height="{h:.2f}" rx="10" fill="#181825"/>',
        ]
        # Window chrome, so a screenshot reads as a terminal rather than as a
        # box of coloured text dropped into the page.
        for i, dot in enumerate(("#f38ba8", "#f9e2af", "#a6e3a1")):
            out.append(
                f'<circle cx="{PAD + 6 + i * 15:.1f}" cy="{PAD + 1:.1f}" '
                f'r="5" fill="{dot}"/>'
            )
        out.append(
            f'<text x="{w / 2:.1f}" y="{PAD + 5:.1f}" fill="#7f849c" '
            f'font-size="{FONT_SIZE * 0.72:.1f}" text-anchor="middle">'
            f"{xml_escape(title)}</text>"
        )
        top = PAD + CHROME_H
        out.append(
            f'<rect x="{PAD:.2f}" y="{top:.2f}" '
            f'width="{CELL_W * self.cols:.2f}" height="{CELL_H * self.rows:.2f}" '
            f'fill="#{DEFAULT_BG}"/>'
        )

        for y in range(self.rows):
            row = self.screen.buffer[y]
            y_px = top + y * CELL_H
            # Background runs first: one rect per stretch of identical colour
            # keeps the file small without losing a single cell.
            x = 0
            while x < self.cols:
                bg = resolve(row[x].bg, DEFAULT_BG)
                start = x
                while x < self.cols and resolve(row[x].bg, DEFAULT_BG) == bg:
                    x += 1
                if bg != DEFAULT_BG:
                    out.append(
                        f'<rect x="{PAD + start * CELL_W:.2f}" y="{y_px:.2f}" '
                        f'width="{(x - start) * CELL_W:.2f}" '
                        f'height="{CELL_H:.2f}" fill="#{bg}"/>'
                    )
            # Then the glyphs.
            base = y_px + CELL_H * 0.76
            x = 0
            while x < self.cols:
                cell = row[x]
                ch = cell.data
                if not ch or ch == " ":
                    x += 1
                    continue
                fg = resolve(cell.fg, DEFAULT_FG)
                bold = cell.bold
                run: list[str] = []
                run_start = x
                wide_seen = False
                while x < self.cols:
                    c = row[x]
                    if (
                        not c.data
                        or c.data == " "
                        or resolve(c.fg, DEFAULT_FG) != fg
                        or c.bold != bold
                    ):
                        break
                    run.append(c.data)
                    if char_width(c.data) == 2:
                        wide_seen = True
                    x += 1
                weight = ' font-weight="bold"' if bold else ""
                if wide_seen:
                    # Place each glyph at its own cell: a double-width glyph
                    # advances two cells but is not two ems wide, so letting
                    # the font flow the run would drift out of the grid.
                    col = run_start
                    for c in run:
                        out.append(
                            f'<text x="{PAD + col * CELL_W:.2f}" y="{base:.2f}" '
                            f'fill="#{fg}"{weight} textLength="{char_width(c) * CELL_W:.2f}" '
                            f'lengthAdjust="spacingAndGlyphs">{xml_escape(c)}</text>'
                        )
                        col += char_width(c)
                else:
                    out.append(
                        f'<text x="{PAD + run_start * CELL_W:.2f}" y="{base:.2f}" '
                        f'fill="#{fg}"{weight} textLength="{(x - run_start) * CELL_W:.2f}" '
                        f'lengthAdjust="spacingAndGlyphs" xml:space="preserve">'
                        f"{xml_escape(''.join(run))}</text>"
                    )
        out.append("</svg>")
        return "\n".join(out)


def prepare_home(home: Path, workspace: Path, lang: str) -> None:
    """A throwaway config dir, so capturing never touches real settings."""
    cfg = home / ".config" / "git-dashboard"
    if home.exists():
        shutil.rmtree(home)
    cfg.mkdir(parents=True)
    repos = [
        ("api-gateway", "platform"),
        ("billing-service", "payments"),
        ("infra-terraform", "platform"),
        ("web-frontend", "storefront"),
        ("design-system", "storefront"),
    ]
    (cfg / "config.json").write_text(
        json.dumps(
            [
                {"name": n, "path": str(workspace / n), "group": g}
                for n, g in repos
            ],
            indent=2,
        ),
        encoding="utf-8",
    )
    (cfg / "prefs.json").write_text(
        json.dumps(
            {
                "theme": "mocha",
                "language": "Japanese" if lang == "ja" else "English",
                "sidebar_collapsed": False,
                "sidebar_width": 260.0,
                "editor_command": "code",
                "diff_ignore_whitespace": False,
                "diff_full_file": False,
                "diff_show_blame": False,
                "recent_compares": [],
                "diff_command": "",
                "repo_sort": 0,
                "auto_refresh_secs": 0,
            }
        ),
        encoding="utf-8",
    )


# Each scene: (file stem, window title, steps, size). A step is a callable
# taking the session, so a scene can click as well as type. `size` is
# (cols, rows) — a five-row repository list framed in a help-screen-sized
# window is mostly empty space, and empty space reads as "nothing here".
def scenes(lang: str):
    ja = lang == "ja"

    def s(keys, wait=0.8):
        return lambda t: t.send(keys, wait)

    def click(x, y, wait=0.8):
        return lambda t: t.click(x, y, wait)

    def wait(sec):
        return lambda t: t.pump(sec)

    open_repo = [s("/", 0.4), s("web-frontend", 0.4), s("\r", 0.6), s("\r", 2.0)]
    open_api = [s("/", 0.4), s("api-gateway", 0.4), s("\r", 0.6), s("\r", 2.0)]

    def sort_home(t):
        label = "最終コミット" if lang == "ja" else "Last commit"
        for y, line in enumerate(t.screen.display):
            if label in line and ("Branch" in line or "ブランチ" in line):
                t.click(wcswidth(line[:line.index(label)]), y)
                return
        raise RuntimeError("Home header was not rendered")

    WIDE = (132, 17)
    TALL = (132, 26)
    return [
        (
            "home",
            "git-dashboard-tui — 5 repositories",
            [wait(3.0)],
            TALL,
        ),
        (
            "home-attention",
            "git-dashboard-tui — needs attention (n)",
            [wait(3.0), s("n", 1.0)],
            TALL,
        ),
        (
            "home-sorted",
            "git-dashboard-tui — sort by clicking a column header",
            [wait(3.0), sort_home],
            TALL,
        ),
        (
            "repo-status",
            "git-dashboard-tui — Status tab",
            [wait(3.0), *open_repo, s("1", 1.2)],
            TALL,
        ),
        (
            "repo-commits",
            "git-dashboard-tui — Commits tab",
            [wait(3.0), *open_repo, s("2", 1.2)],
            TALL,
        ),
        (
            "repo-compare",
            "git-dashboard-tui — pick a compare base and target",
            [
                wait(3.0), *open_repo, s("2", 1.2),
                click(3, 5, 0.6), click(3, 7, 0.9),
            ],
            TALL,
        ),
        (
            "repo-branches",
            "git-dashboard-tui — Branches tab",
            [wait(3.0), *open_repo, s("3", 1.2)],
            (132, 18),
        ),
        (
            "repo-tags",
            "git-dashboard-tui — Tags tab",
            [
                wait(3.0), s("/", 0.4), s("api-gateway", 0.4), s("\r", 0.6),
                s("\r", 2.0), s("4", 1.2),
            ],
            (132, 18),
        ),
        (
            "repo-stash",
            "git-dashboard-tui — Stash tab",
            [
                wait(3.0), s("/", 0.4), s("billing", 0.4), s("\r", 0.6),
                s("\r", 2.0), s("5", 1.2),
            ],
            (132, 18),
        ),
        (
            "repo-contributors",
            "git-dashboard-tui — Contributors tab",
            [wait(3.0), *open_repo, s("6", 1.5)],
            (132, 18),
        ),
        (
            "repo-worktrees",
            "git-dashboard-tui — Worktrees tab",
            [
                wait(3.0), s("/", 0.4), s("design", 0.4), s("\r", 0.6),
                s("\r", 2.0), s("7", 1.2),
            ],
            (132, 18),
        ),
        (
            "diff",
            "git-dashboard-tui — commit diff",
            [wait(3.0), *open_api, s("2", 1.2), s("i", 2.4)],
            TALL,
        ),
        (
            "diff-blame",
            "git-dashboard-tui — blame gutter (b)",
            [wait(3.0), *open_api, s("2", 1.2), s("i", 2.4), s("\r", 1.8), s("b", 2.2)],
            TALL,
        ),
        (
            "commit-search",
            "git-dashboard-tui — cross-repository commit search",
            [wait(3.0), s("S", 0.6), s("timeout" if not ja else "timeout", 0.5), s("\r", 2.5)],
            (132, 20),
        ),
        (
            "global-members",
            "git-dashboard-tui — cross-repository contributors",
            [wait(3.0), s("M", 3.0)],
            TALL,
        ),
        (
            "open-tools",
            "git-dashboard-tui — open in your work tools (O)",
            [wait(3.0), s("O")],
            TALL,
        ),
        (
            "workspace",
            "git-dashboard-tui — local worktrees, notes and favorites (W)",
            [wait(3.0), s("W", 3.0), s("/"), s("design-system-2.0\r"), s("m"),
             s(("明日のレビュー" if lang == "ja" else "Review tomorrow") + "\r"), s("*"),
             s("/"), s("\x7f" * len("design-system-2.0") + "\r")],
            TALL,
        ),
        (
            "settings",
            "git-dashboard-tui — Settings",
            [wait(3.0), s("s", 1.2)],
            TALL,
        ),
        (
            "help",
            "git-dashboard-tui — Help (scrollable)",
            [wait(3.0), s("?", 1.0)],
            (112, 34),
        ),
        (
            "progress",
            "git-dashboard-tui — a fetch in flight",
            # Sampled mid-flight: the spinner and the job count are the point.
            [wait(3.0), lambda t: (os.write(t.fd, b"F"), t.pump(0.25))],
            WIDE,
        ),
    ]


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--bin", default="target/release/git-dashboard-tui")
    ap.add_argument("--out", default="docs/img")
    ap.add_argument("--workspace", default="/tmp/gdt-demo")
    ap.add_argument("--home", default="/tmp/gdt-shot-home")
    ap.add_argument("--lang", choices=["en", "ja"], default="en")
    ap.add_argument("--only", default=None, help="capture one scene by name")
    args = ap.parse_args()

    binary = Path(args.bin).resolve()
    if not binary.exists():
        raise SystemExit(f"no binary at {binary} — cargo build --release first")
    workspace = Path(args.workspace)
    if not workspace.exists():
        raise SystemExit(f"no demo workspace at {workspace} — run docs/gen_demo.py")

    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    suffix = "" if args.lang == "en" else ".ja"

    written = []
    for name, title, steps, size in scenes(args.lang):
        if args.only and args.only != name:
            continue
        cols, rows = size
        home = Path(args.home)
        prepare_home(home, workspace, args.lang)
        # A fresh process per scene: the alternative is threading state
        # between shots, where one scene's leftover filter silently rewrites
        # the next one's screenshot.
        session = Session(binary, home, cols, rows)
        try:
            for step in steps:
                step(session)
            svg = session.to_svg(title)
        finally:
            session.close()
        path = out / f"{name}{suffix}.svg"
        path.write_text(svg, encoding="utf-8")
        written.append((path, len(svg)))
        print(f"  {path}  ({len(svg) // 1024} KB)")

    # A blank capture means the app never drew: fail loudly rather than
    # committing a page of empty boxes.
    for path, size in written:
        if size < 4000:
            raise SystemExit(f"{path} looks empty ({size} bytes)")
    print(f"{len(written)} screenshots -> {out}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
