#!/usr/bin/env python3
# tools/update_rustdoc.py
from pathlib import Path
import re
import sys

def find_project_root(start: Path) -> Path:
    """
    Walk upwards from `start` until a project root marker is found.
    Markers: .git OR (README.md AND src/).
    """
    cur = start.resolve()
    while True:
        if (cur / ".git").exists():
            return cur
        if (cur / "README.md").exists() and (cur / "src").exists():
            return cur
        if cur.parent == cur:
            # Reached filesystem root — fallback to original start
            return start
        cur = cur.parent

def main() -> int:
    # Prefer the current working directory; fall back to the script directory
    cwd = Path.cwd()
    script_dir = Path(__file__).resolve().parent

    root = find_project_root(cwd)
    if not (root / "README.md").exists():
        root = find_project_root(script_dir)

    README = root / "README.md"
    OUTDIR = root / "src"
    OUTFILE = OUTDIR / "RUSTDOC.md"

    if not README.exists():
        print(f"Error: {README} not found (root: {root})", file=sys.stderr)
        return 1

    text = README.read_text(encoding="utf-8")

    # Locate the "## Modules" section header (exact line)
    m = re.search(r'(?m)^##\s+Modules\s*$', text)
    if not m:
        # If not present, copy README as-is
        OUTDIR.mkdir(parents=True, exist_ok=True)
        OUTFILE.write_text(text, encoding="utf-8")
        print("Section '## Modules' not found. Copied README to src/RUSTDOC.md unchanged.")
        return 0

    start_idx = m.end()

    # Section ends at the next level-1 or level-2 heading ("# " or "## ")
    m_end = re.search(r'(?m)^(?:#|\##)\s+', text[start_idx:])
    end_idx = start_idx + m_end.start() if m_end else len(text)

    before = text[:start_idx]
    section = text[start_idx:end_idx]
    after = text[end_idx:]

    # Inside the section: replace inline links [label](url) -> [label]
    # Exclude images via negative lookbehind for '!'
    section_fixed = re.sub(
        r'(?<!\!)\[(?P<label>[^\]]+)\]\([^)]+\)',
        r'[\g<label>]',
        section
    )

    new_text = before + section_fixed + after

    OUTDIR.mkdir(parents=True, exist_ok=True)
    OUTFILE.write_text(new_text, encoding="utf-8")
    print(f"Created {OUTFILE} (project root: {root}) with Docs.rs style links in '## Modules' section.")
    return 0

if __name__ == "__main__":
    raise SystemExit(main())
