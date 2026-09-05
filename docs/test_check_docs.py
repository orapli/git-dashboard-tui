"""Regression checks that the docs gate rejects real documentation regressions."""
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


class DocumentationGateTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        shutil.copytree(ROOT / 'docs', self.root / 'docs', ignore=shutil.ignore_patterns('__pycache__'))
        for name in ('README.md', 'README.ja.md', 'CLAUDE.md', 'AGENTS.md', 'CHANGELOG.md', 'SECURITY.md', 'Cargo.toml', 'LICENSE', 'install.sh'):
            shutil.copy2(ROOT / name, self.root / name)
        shutil.copytree(ROOT / '.github', self.root / '.github')
        shutil.copytree(ROOT / 'src', self.root / 'src')
        shutil.copy2(ROOT / 'BACKLOG.md', self.root / 'BACKLOG.md')
        # Existing authored documents link to generated wiki context. Copy it read-only.
        shutil.copytree(ROOT / 'openwiki', self.root / 'openwiki')

    def check_failure(self, expected):
        result = subprocess.run([sys.executable, 'docs/check_docs.py'], cwd=self.root, capture_output=True, text=True)
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn(expected, result.stderr)

    def test_stale_generated_html(self):
        with (self.root / 'docs/manual.html').open('a') as file:
            file.write('\n<!-- stale -->\n')
        self.check_failure('out of date')

    def test_broken_published_anchor(self):
        with (self.root / 'README.md').open('a') as file:
            file.write('\n[Broken](https://orapli.github.io/git-dashboard-tui/manual.html#missing-section)\n')
        self.check_failure('missing anchor')

    def test_missing_translated_image(self):
        (self.root / 'docs/img/home.ja.svg').unlink()
        self.check_failure('missing bilingual image partner')

    def test_language_structure_drift(self):
        with (self.root / 'docs/reference.ja.md').open('a') as file:
            file.write('\n## 日本語のみの追加\n')
        self.check_failure('heading levels differ')

    def test_valid_checkout(self):
        result = subprocess.run([sys.executable, 'docs/check_docs.py'], cwd=self.root, capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)


if __name__ == '__main__':
    unittest.main()
