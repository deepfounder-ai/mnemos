#!/usr/bin/env python3
"""
Validate that every page under examples/pages/ parses as YAML
frontmatter and obeys the spec in docs/page-format.md.

This is a standalone checker that uses only PyYAML (no mnemos
runtime). It is intentionally simple — the production validator
lives in src/core/frontmatter.rs and uses serde + serde_yaml.
This script exists so the docs/examples tree can be checked
without building the Rust binary.

Run:
    python3 scripts/validate-frontmatter.py
    # or:
    python3 scripts/validate-frontmatter.py path/to/page.md
"""

from __future__ import annotations

import re
import sys
from pathlib import Path
from typing import Any

import yaml

PAGE_TYPE_ENUM = {"concept", "recipe", "reference", "decision"}
SCOPE_ENUM = {"global", "local"}
SOURCE_TYPE_ENUM = {"url", "session", "file"}
SLUG_RE = re.compile(r"^[a-z0-9][a-z0-9-]{0,78}[a-z0-9]$")
DATE_RE = re.compile(r"^\d{4}-\d{2}-\d{2}$")
TAG_RE = re.compile(r"^[a-z0-9][a-z0-9-]*[a-z0-9]$")


def split_frontmatter(path: Path) -> tuple[dict[str, Any], str]:
    text = path.read_text(encoding="utf-8")
    if not text.startswith("---\n"):
        raise ValueError(f"{path}: missing opening '---' on line 1")
    # Find closing ---
    end = text.find("\n---", 4)
    if end < 0:
        raise ValueError(f"{path}: missing closing '---'")
    yaml_block = text[4:end]
    body = text[end + 4 :]
    if body.startswith("\n"):
        body = body[1:]
    try:
        data = yaml.safe_load(yaml_block)
    except yaml.YAMLError as e:
        raise ValueError(f"{path}: YAML parse error: {e}") from e
    if not isinstance(data, dict):
        raise ValueError(f"{path}: frontmatter must be a mapping")
    return data, body


def validate_one(path: Path) -> list[str]:
    errors: list[str] = []
    try:
        fm, body = split_frontmatter(path)
    except ValueError as e:
        return [str(e)]

    # Required string fields
    for f in ("title", "created", "updated", "scope", "page_type"):
        if f not in fm:
            errors.append(f"{path}: missing required field '{f}'")

    if "title" in fm and not isinstance(fm["title"], str):
        errors.append(f"{path}: 'title' must be a string")
    elif fm.get("title", "").endswith("."):
        errors.append(f"{path}: 'title' must not end with '.'")

    if "created" in fm and not DATE_RE.match(str(fm["created"])):
        errors.append(f"{path}: 'created' must be YYYY-MM-DD")
    if "updated" in fm and not DATE_RE.match(str(fm["updated"])):
        errors.append(f"{path}: 'updated' must be YYYY-MM-DD")
    if (
        "created" in fm
        and "updated" in fm
        and str(fm["updated"]) < str(fm["created"])
    ):
        errors.append(f"{path}: 'updated' must be >= 'created'")

    # tags
    if "tags" not in fm:
        errors.append(f"{path}: missing required field 'tags'")
    else:
        tags = fm["tags"]
        if not isinstance(tags, list):
            errors.append(f"{path}: 'tags' must be a list")
        else:
            if not (1 <= len(tags) <= 8):
                errors.append(f"{path}: 'tags' must have 1-8 entries, got {len(tags)}")
            for t in tags:
                if not isinstance(t, str):
                    errors.append(f"{path}: tag must be a string, got {type(t).__name__}")
                    continue
                if not TAG_RE.match(t):
                    errors.append(f"{path}: tag '{t}' must be lowercase kebab-case")

    # scope
    if "scope" in fm and fm["scope"] not in SCOPE_ENUM:
        errors.append(
            f"{path}: 'scope' must be one of {sorted(SCOPE_ENUM)}, got {fm['scope']!r}"
        )

    # page_type
    if "page_type" in fm and fm["page_type"] not in PAGE_TYPE_ENUM:
        errors.append(
            f"{path}: 'page_type' must be one of {sorted(PAGE_TYPE_ENUM)}, "
            f"got {fm['page_type']!r}"
        )

    # sources
    if "sources" in fm:
        sources = fm["sources"]
        if not isinstance(sources, list):
            errors.append(f"{path}: 'sources' must be a list")
        elif not sources:
            errors.append(f"{path}: 'sources' must have at least one entry")
        else:
            for i, s in enumerate(sources):
                if not isinstance(s, dict):
                    errors.append(f"{path}: sources[{i}] must be a mapping")
                    continue
                t = s.get("type")
                if t not in SOURCE_TYPE_ENUM:
                    errors.append(
                        f"{path}: sources[{i}].type must be one of "
                        f"{sorted(SOURCE_TYPE_ENUM)}, got {t!r}"
                    )
                if "ref" not in s or not isinstance(s.get("ref"), str) or not s["ref"].strip():
                    errors.append(f"{path}: sources[{i}].ref must be a non-empty string")
                if t == "url" and not s.get("origin"):
                    errors.append(
                        f"{path}: sources[{i}] is type=url and should have 'origin'"
                    )

    # related
    if "related" in fm:
        related = fm["related"]
        if not isinstance(related, list):
            errors.append(f"{path}: 'related' must be a list")
        else:
            for r in related:
                if not isinstance(r, str):
                    errors.append(f"{path}: related[] entries must be strings")
                    continue
                if not SLUG_RE.match(r):
                    errors.append(
                        f"{path}: related[] entry '{r}' is not a valid slug"
                    )

    # body must be non-empty
    if not body.strip():
        errors.append(f"{path}: body must be non-empty")

    return errors


def main(argv: list[str]) -> int:
    repo_root = Path(__file__).resolve().parent.parent
    if len(argv) > 1:
        targets = [Path(p) for p in argv[1:]]
    else:
        targets = sorted((repo_root / "examples" / "pages").glob("*.md"))

    if not targets:
        print("no pages to validate", file=sys.stderr)
        return 1

    all_errors: list[str] = []
    for p in targets:
        all_errors.extend(validate_one(p))

    if all_errors:
        for e in all_errors:
            print(f"FAIL: {e}", file=sys.stderr)
        print(f"\n{len(all_errors)} validation error(s) across {len(targets)} page(s)", file=sys.stderr)
        return 1

    print(f"OK: {len(targets)} page(s) validated")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
