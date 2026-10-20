#!/usr/bin/env python3
# Copyright © 2026 Tess Dev Team <https://tessheadless.com>
# SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
# Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

"""Copies example files into the docs pages that show them.

A page marks the place with `{/* include: path/from/repo/root */}` (an MDX
comment); the code
block that follows the marker is replaced with the file's current content.

    scripts/sync-doc-examples.py           # update the pages
    scripts/sync-doc-examples.py --check   # fail if a page is out of date (CI)
"""

import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
MARKER = re.compile(r"(\{/\* include: (?P<path>\S+) \*/\}\n)(?:\n```[^\n]*\n.*?\n```\n)?", re.S)
LANG = {".mjs": "js", ".js": "js", ".ts": "ts", ".yaml": "yaml", ".sh": "bash", ".py": "python"}


def render(match: re.Match) -> str:
    path = ROOT / match["path"]
    code = path.read_text().rstrip("\n")
    lang = LANG.get(path.suffix, "")
    return f'{match[1]}\n```{lang} title="{path.name}"\n{code}\n```\n'


def main() -> int:
    check = "--check" in sys.argv
    stale = []
    for page in sorted((ROOT / "docs" / "docs").rglob("*.md")):
        text = page.read_text()
        updated = MARKER.sub(render, text)
        if updated != text:
            stale.append(page.relative_to(ROOT))
            if not check:
                page.write_text(updated)
    for page in stale:
        print(f"{'out of date' if check else 'updated'}: {page}")
    return 1 if check and stale else 0


if __name__ == "__main__":
    sys.exit(main())
