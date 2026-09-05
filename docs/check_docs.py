#!/usr/bin/env python3
"""Check authored docs, generated HTML, local links, SVGs and bilingual structure.

No network or third-party dependencies. External site availability is not checked.
Run from any directory: python3 docs/check_docs.py
"""
from __future__ import annotations

from collections import Counter
from html.parser import HTMLParser
from pathlib import Path
import re
import sys
from urllib.parse import unquote, urlsplit
import xml.etree.ElementTree as ET

import gen_manual

ROOT = Path(__file__).resolve().parent.parent
ERRORS: list[str] = []


def error(path: Path, message: str) -> None:
    ERRORS.append(f"{path.relative_to(ROOT)}: {message}")


class Page(HTMLParser):
    def __init__(self, text: str):
        super().__init__(convert_charrefs=True)
        self.ids: list[str] = []
        self.links: list[str] = []
        self.images: list[str] = []
        self.feed(text)

    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        if 'id' in attrs:
            self.ids.append(attrs['id'])
        for attr in ('href', 'src'):
            if attr in attrs:
                self.links.append(attrs[attr])
        if tag == 'img':
            self.images.append(attrs.get('src', ''))
            if not attrs.get('alt', '').strip():
                self.images.append('MISSING_ALT')


def markdown(text: str) -> str:
    # Code examples may contain illustrative paths and URLs; do not treat them as links.
    return re.sub(r'^```[^\n]*\n.*?^```\s*$', '', text, flags=re.M | re.S)


def anchors(path: Path) -> set[str]:
    if path.suffix == '.svg':
        return {e.attrib['id'] for e in ET.parse(path).iter() if 'id' in e.attrib}
    text = path.read_text(encoding='utf-8')
    ids = set(Page(text).ids)
    if path.suffix == '.md':
        used: Counter = Counter()
        for heading in re.findall(r'^#{1,6}\s+(.+?)\s*#*$', markdown(text), re.M):
            slug = re.sub(r'<[^>]*>', '', heading).lower()
            slug = re.sub(r'[^\w\-\s]', '', slug).replace(' ', '-')
            count = used[slug]
            used[slug] += 1
            ids.add(f'{slug}-{count}' if count else slug)
    return ids


def local_target(source: Path, link: str):
    url = urlsplit(link)
    if url.scheme in ('mailto', 'data', 'tel'):
        return None
    if url.netloc:
        mappings = {
            'orapli.github.io': ('/git-dashboard-tui/', ROOT / 'docs'),
            'github.com': ('/orapli/git-dashboard-tui/blob/main/', ROOT),
            'raw.githubusercontent.com': ('/orapli/git-dashboard-tui/main/', ROOT),
        }
        match = mappings.get(url.netloc)
        if not match or not url.path.startswith(match[0]):
            return None
        target = match[1] / unquote(url.path[len(match[0]):])
    elif url.scheme:
        return None
    else:
        target = source.parent / unquote(url.path) if url.path else source
    if target.is_dir():
        target = target / 'index.html'
    return target.resolve(), unquote(url.fragment)


def check_links(path: Path):
    text = path.read_text(encoding='utf-8')
    page = Page(markdown(text) if path.suffix == '.md' else text)
    links = page.links
    if path.suffix == '.md':
        links += re.findall(r'!?\[[^\]\n]*\]\(([^\s)]+)\)', markdown(text))
        links += re.findall(r'^\[[^\]]+\]:\s*(\S+)', markdown(text), re.M)
    for link in links:
        found = local_target(path, link)
        if found is None:
            continue
        target, fragment = found
        if not target.is_relative_to(ROOT):
            error(path, f'link escapes repository: {link}')
        elif not target.exists():
            error(path, f'missing link/image: {link}')
        elif fragment and target.suffix in ('.md', '.html', '.svg') and fragment not in anchors(target):
            error(path, f'missing anchor: {link}')
    if 'MISSING_ALT' in page.images:
        error(path, 'image is missing alt text')
    if len(page.ids) != len(set(page.ids)):
        error(path, 'duplicate HTML ids')


def normalize(value: str) -> str:
    return value.replace('.ja.', '.')


def check_pair(en: Path, ja: Path):
    a, b = en.read_text(encoding='utf-8'), ja.read_text(encoding='utf-8')
    # Structure and destinations are machine-checkable; translation meaning needs review.
    for label, pattern in (
        ('heading levels', r'^(#{1,6}) '),
        ('table row count', r'^(\|)'),
        ('code block languages', r'^```([^\n]*)'),
    ):
        if re.findall(pattern, a, re.M) != re.findall(pattern, b, re.M):
            error(ja, f'{label} differ from {en.name}')
    destinations = lambda text: Counter(normalize(x).split('#')[0] for x in re.findall(r'\]\(([^\s)]+)\)', text))
    if destinations(a) != destinations(b):
        error(ja, f'link/image destinations differ from {en.name}')


def main() -> int:
    for lang, name in ((gen_manual.EN, 'manual.html'), (gen_manual.JA, 'manual.ja.html')):
        p = ROOT / 'docs' / name
        expected = gen_manual.render(lang)
        if not p.exists() or p.read_text(encoding='utf-8') != expected:
            error(p, 'out of date; run python3 docs/gen_manual.py')
    index = ROOT / 'docs/index.html'
    if not index.exists() or index.read_text(encoding='utf-8') != gen_manual.render_index():
        error(index, 'out of date; run python3 docs/gen_manual.py')
    ids = [sid for sid, _, _ in gen_manual.SECTIONS]
    if len(ids) != len(set(ids)):
        error(ROOT / 'docs/gen_manual.py', 'duplicate section ids')

    def bilingual(value):
        if isinstance(value, tuple) and len(value) == 2 and all(isinstance(s, str) for s in value):
            if not all(s.strip() for s in value):
                error(ROOT / 'docs/gen_manual.py', 'empty bilingual text')
        elif isinstance(value, (list, tuple)):
            for child in value:
                bilingual(child)
    for _, title, blocks in gen_manual.SECTIONS:
        bilingual(title)
        bilingual(blocks)
    en = Page(gen_manual.render(gen_manual.EN))
    ja = Page(gen_manual.render(gen_manual.JA))
    if en.ids != ja.ids or list(map(normalize, en.images)) != list(map(normalize, ja.images)):
        error(ROOT / 'docs/manual.ja.html', 'manual section/image parity mismatch')
    authored = [ROOT / n for n in ('README.md', 'README.ja.md', 'CLAUDE.md', 'AGENTS.md', 'CHANGELOG.md', 'SECURITY.md')]
    authored += sorted((ROOT / 'docs').glob('*.md')) + sorted((ROOT / 'docs').glob('*.html'))
    for path in authored:
        if path.exists():
            check_links(path)
    for path in sorted((ROOT / 'docs/img').glob('*.svg')):
        try:
            ET.parse(path)
        except ET.ParseError as exc:
            error(path, f'invalid SVG XML: {exc}')
        partner = path.with_name(path.name.replace('.ja.svg', '.svg') if '.ja.svg' in path.name else path.stem + '.ja.svg')
        if not partner.exists():
            error(path, f'missing bilingual image partner: {partner.name}')
    check_pair(ROOT / 'README.md', ROOT / 'README.ja.md')
    check_pair(ROOT / 'docs/reference.md', ROOT / 'docs/reference.ja.md')
    # Keep the stated MSRV in sync with the package and dedicated CI job.
    import tomllib
    msrv = tomllib.loads((ROOT / 'Cargo.toml').read_text())['package']['rust-version']
    for name in ('README.md', 'README.ja.md', 'CLAUDE.md', 'docs/manual_tasks.py'):
        if f'Rust {msrv}' not in (ROOT / name).read_text():
            error(ROOT / name, f'missing Rust {msrv} requirement')
    if f'toolchain: "{msrv}.0"' not in (ROOT / '.github/workflows/ci.yml').read_text():
        error(ROOT / '.github/workflows/ci.yml', 'MSRV job does not match Cargo.toml')
    if ERRORS:
        print('\n'.join(ERRORS), file=sys.stderr)
        return 1
    print(f'Documentation checks passed: {len(authored)} pages, {len(ids)} bilingual manual sections, SVG pairs and MSRV consistency.')
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
