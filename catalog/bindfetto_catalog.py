#!/usr/bin/env python3
"""bindfetto AIDL catalog builder (Track B1).

Turns AIDL into the JSON catalog the offline decoders (CLI, DLT plugin, VS Code
extension) consume:

    { "android.app.IActivityManager": { "1": "getTasks", "7": "startActivity" } }

Transaction codes follow AIDL's own rule: an interface's methods are numbered from
`IBinder.FIRST_CALL_TRANSACTION` (1) in **declaration order**, unless a method fixes
its own code with a trailing `= N`. (AIDL requires either all methods to be explicitly
numbered or none, so mixing is a malformed input we handle best-effort.) The
interface-agnostic special transactions (PING/DUMP/INTERFACE/...) are resolved by the
decoder itself and are not stored in the catalog.

Sources may be any mix of: a local `.aidl` file, a directory (recursed for `*.aidl`),
or an `http(s)://` URL to a `.aidl` file.

Usage:
    bindfetto_catalog.py [-o catalog.json] <source> [<source> ...]
"""
from __future__ import annotations

import argparse
import json
import os
import re
import sys
import urllib.request
from typing import Dict, Iterator, Optional, Tuple

FIRST_CALL_TRANSACTION = 1

Methods = Dict[int, str]
Catalog = Dict[str, Methods]


# --- source loading --------------------------------------------------------------

def load_sources(sources) -> Iterator[Tuple[str, str]]:
    """Yield (label, aidl_text) for every .aidl input across all sources."""
    for src in sources:
        if src.startswith(("http://", "https://")):
            yield src, _fetch_url(src)
        elif os.path.isdir(src):
            for root, _dirs, files in os.walk(src):
                for name in sorted(files):
                    if name.endswith(".aidl"):
                        path = os.path.join(root, name)
                        yield path, _read_file(path)
        elif os.path.isfile(src):
            yield src, _read_file(src)
        else:
            raise SystemExit(f"error: not a file, directory, or http(s) URL: {src}")


def _read_file(path: str) -> str:
    with open(path, "r", encoding="utf-8") as f:
        return f.read()


def _fetch_url(url: str) -> str:
    req = urllib.request.Request(url, headers={"User-Agent": "bindfetto-catalog"})
    with urllib.request.urlopen(req, timeout=30) as resp:  # noqa: S310 (http(s) only)
        return resp.read().decode("utf-8")


# --- AIDL parsing ----------------------------------------------------------------

def _strip_comments(src: str) -> str:
    src = re.sub(r"/\*.*?\*/", " ", src, flags=re.S)  # block comments
    src = re.sub(r"//[^\n]*", "", src)  # line comments
    return src


def _strip_annotations(src: str) -> str:
    # Drop `@Annotation` and an optional `(...)` arg list so annotation braces/parens
    # (e.g. @SuppressWarnings(value={"..."})) don't confuse the statement scanner.
    return re.sub(r"@\w+(\s*\([^)]*\))?", " ", src)


def _package(src: str) -> str:
    m = re.search(r"\bpackage\s+([A-Za-z_][\w.]*)\s*;", src)
    return m.group(1) if m else ""


def _braced_body(src: str, open_idx: int) -> str:
    """Return the text between the `{` at open_idx and its matching `}`."""
    depth = 0
    for j in range(open_idx, len(src)):
        if src[j] == "{":
            depth += 1
        elif src[j] == "}":
            depth -= 1
            if depth == 0:
                return src[open_idx + 1 : j]
    return src[open_idx + 1 :]


def _top_level_statements(body: str) -> Iterator[str]:
    """Split an interface body into statements: `;`-terminated lines at brace depth 0,
    and whole `{...}`-blocks (nested types) as single statements."""
    buf: list[str] = []
    depth = 0
    for c in body:
        if c == "{":
            depth += 1
            buf.append(c)
        elif c == "}":
            depth -= 1
            buf.append(c)
            if depth == 0:
                yield "".join(buf)
                buf = []
        elif c == ";" and depth == 0:
            yield "".join(buf)
            buf = []
        else:
            buf.append(c)
    if "".join(buf).strip():
        yield "".join(buf)


_SKIP = re.compile(r"^\s*(const|interface|parcelable|enum|union)\b")


def _method_name(stmt: str) -> Optional[str]:
    paren = stmt.find("(")
    if paren == -1:
        return None
    m = re.search(r"([A-Za-z_]\w*)\s*$", stmt[:paren])
    return m.group(1) if m else None


def _explicit_code(stmt: str) -> Optional[int]:
    close = stmt.rfind(")")
    if close == -1:
        return None
    m = re.search(r"=\s*(\d+)", stmt[close + 1 :])
    return int(m.group(1)) if m else None


def _parse_methods(body: str) -> Methods:
    methods: Methods = {}
    position = 0  # 0-based declaration index among methods only
    for stmt in _top_level_statements(body):
        s = stmt.strip()
        if not s or _SKIP.match(s) or "(" not in s or ")" not in s:
            continue
        name = _method_name(s)
        if name is None:
            continue
        code = _explicit_code(s)
        if code is None:
            code = FIRST_CALL_TRANSACTION + position
        methods[code] = name
        position += 1
    return methods


def _parse_interfaces(src: str) -> Catalog:
    package = _package(src)
    out: Catalog = {}
    for m in re.finditer(r"\binterface\s+([A-Za-z_]\w*)", src):
        brace = src.find("{", m.end())
        if brace == -1:
            continue
        fq = f"{package}.{m.group(1)}" if package else m.group(1)
        out[fq] = _parse_methods(_braced_body(src, brace))
    return out


# --- driver ----------------------------------------------------------------------

def build_catalog(sources) -> Catalog:
    catalog: Catalog = {}
    for _label, text in load_sources(sources):
        text = _strip_annotations(_strip_comments(text))
        for fq, methods in _parse_interfaces(text).items():
            catalog.setdefault(fq, {}).update(methods)
    return catalog


def to_json(catalog: Catalog) -> str:
    ordered = {
        fq: {str(code): catalog[fq][code] for code in sorted(catalog[fq])}
        for fq in sorted(catalog)
    }
    return json.dumps(ordered, indent=2) + "\n"


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description="Build a bindfetto AIDL catalog (JSON).")
    ap.add_argument(
        "sources",
        nargs="+",
        help="local .aidl file(s), directory(ies) to recurse, or http(s) URL(s)",
    )
    ap.add_argument("-o", "--out", help="write JSON here (default: stdout)")
    args = ap.parse_args(argv)

    catalog = build_catalog(args.sources)
    text = to_json(catalog)

    methods = sum(len(v) for v in catalog.values())
    if args.out:
        with open(args.out, "w", encoding="utf-8") as f:
            f.write(text)
        print(
            f"wrote {methods} methods across {len(catalog)} interfaces to {args.out}",
            file=sys.stderr,
        )
    else:
        sys.stdout.write(text)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
