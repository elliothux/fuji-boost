"""Verify that every dated research file is indexed and every local Markdown link resolves."""
from pathlib import Path
from urllib.parse import unquote, urlsplit
import json
import re

ROOT = Path(__file__).resolve().parent.parent
DOCS = ROOT / 'docs/research'


def destinations(path):
    fence = None
    for line in path.read_text().splitlines():
        match = re.match(r'^\s*(`{3,}|~{3,})', line)
        if match:
            marker = match[1][0]
            if fence == marker:
                fence = None
            elif fence is None:
                fence = marker
            continue
        if fence:
            continue
        for match in re.finditer(r'!?\[[^\]\n]*\]\((<[^>\n]*>|[^)\n]*)\)', line):
            target = match[1].strip()
            yield target[1:-1] if target.startswith('<') else re.split(r'\s+[\'\"]', target, maxsplit=1)[0]


index_targets = {(DOCS / value.split('#')[0]).resolve() for value in destinations(DOCS / 'README.md')}
daily = [p for p in DOCS.glob('*/*.md') if p.parent.name != '_meta']
assert all(p.resolve() in index_targets for p in daily), 'unindexed daily file'
checked = 0
missing = []
for path in [ROOT / 'README.md', ROOT / 'RESEARCH.md', *DOCS.rglob('*.md'), *(ROOT / 'research').rglob('*.md')]:
    for target in destinations(path):
        parsed = urlsplit(target)
        if parsed.scheme or parsed.netloc or not parsed.path:
            continue
        dest = Path(unquote(parsed.path))
        if not dest.is_absolute():
            dest = path.parent / dest
        checked += 1
        if not dest.exists():
            missing.append({'file': str(path.relative_to(ROOT)), 'target': target})
assert not missing, missing
print(json.dumps({'passed': True, 'dateFiles': len(daily), 'localLinks': checked, 'missing': missing}))
