#!/usr/bin/env python3
"""TST-017 heuristic: flag hard-coded hero or item ID lists in Rust source.

Run through scripts/no-hardcoded-ids.sh. See the limits listed there.
Exit status: 0 clean, 1 findings, 2 usage error.
"""
import re
import sys
from pathlib import Path

# Tuning constants for this tooling heuristic only (not app behaviour).
MIN_INT_LIST = 5       # integer literals in one array literal
MIN_TUPLE_TABLE = 3    # (id, "name") tuples in one array literal
MIN_MATCH_ARMS = 3     # `123 => "name"` arms within MATCH_WINDOW lines
MATCH_WINDOW = 12
NAMED_MIN_LITERALS = 2  # literals in an array assigned to a hero/item-named binding
ALLOW = "no-hardcoded-ids-allow"

INT = r"(?:0x[0-9a-fA-F_]+|\d[\d_]*)(?:_?[iu](?:8|16|32|64|128|size))?"
NAME_RE = re.compile(r"hero|item|upgrade|ability", re.I)


def blank_comments_and_strings(text):
    """Blank out comments; keep string literals but mark them as "S" so counting works.

    Returns text of the same length and line structure.
    """
    out = []
    i, n = 0, len(text)
    while i < n:
        c = text[i]
        two = text[i : i + 2]
        if two == "//":
            j = text.find("\n", i)
            j = n if j < 0 else j
            out.append(" " * (j - i))
            i = j
        elif two == "/*":
            depth, j = 1, i + 2
            while j < n and depth:
                if text[j : j + 2] == "/*":
                    depth += 1
                    j += 2
                elif text[j : j + 2] == "*/":
                    depth -= 1
                    j += 2
                else:
                    j += 1
            out.append(re.sub(r"[^\n]", " ", text[i:j]))
            i = j
        elif c == '"':
            j = i + 1
            while j < n and text[j] != '"':
                j += 2 if text[j] == "\\" else 1
            j = min(j + 1, n)
            body = text[i:j]
            out.append('"S' + re.sub(r"[^\n]", " ", body[2:-1]) + '"' if len(body) >= 3 else body)
            i = j
        elif c == "'" and re.match(r"'(?:\\.|[^\\'])'", text[i:]):
            m = re.match(r"'(?:\\.|[^\\'])'", text[i:])
            out.append(" " * len(m.group(0)))
            i += len(m.group(0))
        else:
            out.append(c)
            i += 1
    return "".join(out)


def strip_cfg_test(text):
    """Blank out items annotated #[cfg(test)] (usually `mod tests { ... }`)."""
    out = list(text)
    for m in re.finditer(r"#\[cfg\(test\)\]", text):
        i = m.end()
        # find the end of the item: first `;` before any `{`, or the matching `}`
        j = i
        while j < len(text) and text[j] not in "{;":
            j += 1
        if j >= len(text):
            end = len(text)
        elif text[j] == ";":
            end = j + 1
        else:
            depth = 0
            while j < len(text):
                if text[j] == "{":
                    depth += 1
                elif text[j] == "}":
                    depth -= 1
                    if depth == 0:
                        break
                j += 1
            end = min(j + 1, len(text))
        for k in range(m.start(), end):
            if out[k] != "\n":
                out[k] = " "
    return "".join(out)


def is_test_file(path):
    parts = path.parts
    name = path.name
    return (
        "tests" in parts
        or "benches" in parts
        or name in ("tests.rs", "test.rs")
        or name.endswith("_test.rs")
        or name.endswith("_tests.rs")
    )


def split_top(body):
    """Split on commas that are not inside (), {} or <> so nested calls and tuples count once."""
    parts, depth, cur = [], 0, []
    for ch in body:
        if ch in "({":
            depth += 1
        elif ch in ")}":
            depth -= 1
        if ch == "," and depth == 0:
            parts.append("".join(cur))
            cur = []
        else:
            cur.append(ch)
    parts.append("".join(cur))
    return parts


def line_of(text, pos):
    return text.count("\n", 0, pos) + 1


def check_file(path):
    raw = path.read_text(encoding="utf-8", errors="replace")
    raw_lines = raw.splitlines()
    text = strip_cfg_test(blank_comments_and_strings(raw))
    findings = []

    def add(pos, why):
        ln = line_of(text, pos)
        # allow marker on the finding's line or the line above (in the raw text, comments kept)
        window = raw_lines[max(0, ln - 2) : ln]
        if any(ALLOW in w for w in window):
            return
        findings.append((ln, why))

    # 1. Array literals: [ ... ] not preceded by an identifier or closing bracket (indexing).
    for m in re.finditer(r"(?<![\w\)\]])\[([^\[\]]*)\]", text):
        body = m.group(1)
        items = [x.strip() for x in split_top(body) if x.strip()]
        ints = [x for x in items if re.fullmatch(INT, x)]
        if len(ints) >= MIN_INT_LIST:
            add(m.start(), f"array of {len(ints)} integer literals (possible ID list)")
            continue
        tuples = re.findall(r"\(\s*" + INT + r'\s*,\s*"S', body)
        if len(tuples) >= MIN_TUPLE_TABLE:
            add(m.start(), f"table of {len(tuples)} (id, \"name\") tuples")

    # 2. match arms `123 => "name"` (an ID-to-name lookup written as a match)
    arm_lines = [
        i + 1
        for i, ln in enumerate(text.splitlines())
        if re.match(r'\s*' + INT + r'(?:\s*\|\s*' + INT + r')*\s*=>\s*(?:Some\()?\s*"S', ln)
    ]
    for a in range(len(arm_lines)):
        window = [x for x in arm_lines if arm_lines[a] <= x < arm_lines[a] + MATCH_WINDOW]
        if len(window) >= MIN_MATCH_ARMS:
            pos = sum(len(l) + 1 for l in text.splitlines()[: arm_lines[a] - 1])
            add(pos, f"{len(window)} `id => \"name\"` match arms (ID lookup in code)")
            break

    # 3. hero/item-named const, static or binding assigned an array/vec of literals
    for m in re.finditer(
        r"\b(?:const|static|let(?:\s+mut)?)\s+(\w+)\s*(?::[^=]*)?=\s*(?:&\s*)?(?:vec!\s*)?\[([^\[\]]*)\]",
        text,
    ):
        if not NAME_RE.search(m.group(1)):
            continue
        lits = [x for x in split_top(m.group(2)) if re.fullmatch(r'\s*(?:' + INT + r'|"S[^"]*")\s*', x)]
        if len(lits) >= NAMED_MIN_LITERALS:
            add(m.start(), f"`{m.group(1)}` is a hard-coded list of {len(lits)} literals")

    return findings


def main(argv):
    roots = [Path(a) for a in argv[1:]] or [Path("crates")]
    files = []
    for r in roots:
        if r.is_file():
            files.append(r)
        else:
            files += sorted(r.glob("*/src/**/*.rs")) if r.name == "crates" else sorted(r.rglob("*.rs"))
    total = 0
    checked = 0
    for f in files:
        if is_test_file(f):
            continue
        checked += 1
        for ln, why in check_file(f):
            print(f"FAIL {f}:{ln}: {why}")
            total += 1
    print(f"no-hardcoded-ids: {checked} files checked, {total} finding(s)")
    return 1 if total else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
