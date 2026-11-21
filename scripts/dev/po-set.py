#!/usr/bin/env python3
"""Set translations in one of Canto's po files from a JSON file.

Usage: scripts/dev/po-set.py LANG PAIRS.json [--po FILE]
  LANG        a code from po/LINGUAS, such as pt-br or de (the file is po/LANG.po)
  PAIRS.json  a JSON object that maps msgid -> msgstr
  --po FILE   write to this po file instead (for a test copy)

Run it after `ninja -C build canto-pot` and `ninja -C build canto-update-po`, so every
msgid is in the file. It clears the fuzzy flag of the entries it sets and lists the
msgids it did not find. It refuses, and writes nothing, when a msgid belongs to a
plural entry (msgid_plural: set its msgstr[n] lines by hand) or to more than one entry
(the same text in different contexts), or when a msgstr is empty or not a string.
Exit code: 0 when every pair was set, 1 when some msgid was missing, 2 on a refusal.
"""
import json
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]


def unquote(lines):
    """The text of a run of PO string lines ("..." on each line)."""
    return "".join(json.loads(line) for line in lines)


def quote(text):
    """One PO string line for text."""
    out = text.replace("\\", "\\\\").replace('"', '\\"')
    out = out.replace("\n", "\\n").replace("\t", "\\t").replace("\r", "\\r")
    return '"' + out + '"'


def strip_fuzzy(head):
    lines = []
    for line in head.split("\n"):
        if line.startswith("#,"):
            flags = [f.strip() for f in line[2:].split(",") if f.strip() and f.strip() != "fuzzy"]
            if not flags:
                continue
            line = "#, " + ", ".join(flags)
        lines.append(line)
    return "\n".join(lines)


def main(argv):
    args = argv[1:]
    po_path = None
    if "--po" in args:
        i = args.index("--po")
        if i + 1 >= len(args):
            sys.exit("--po needs a file")
        po_path = Path(args[i + 1])
        del args[i:i + 2]
    if len(args) != 2:
        print(__doc__.strip(), file=sys.stderr)
        return 2
    lang, pairs_file = args
    if po_path is None:
        linguas = (REPO / "po" / "LINGUAS").read_text(encoding="utf-8").split()
        if lang not in linguas:
            print(f"{lang} is not in po/LINGUAS", file=sys.stderr)
            return 2
        po_path = REPO / "po" / f"{lang}.po"

    pairs = json.loads(Path(pairs_file).read_text(encoding="utf-8"))
    if not isinstance(pairs, dict):
        print("the JSON file must hold one object: msgid -> msgstr", file=sys.stderr)
        return 2
    bad = [k for k, v in pairs.items() if not isinstance(v, str) or not v]
    if bad:
        print(f"refused, nothing written: empty or non-string msgstr for {bad}", file=sys.stderr)
        return 2

    text = po_path.read_text(encoding="utf-8")
    blocks = text.split("\n\n")
    entry = re.compile(r'^msgid ("[^\n]*"(?:\n"[^\n]*")*)\n(msgid_plural |msgstr )', re.M)

    found = {}
    plural = set()
    for i, block in enumerate(blocks):
        m = entry.search(block)
        if not m:
            continue
        msgid = unquote(m.group(1).split("\n"))
        if msgid not in pairs:
            continue
        if m.group(2) == "msgid_plural ":
            plural.add(msgid)
        found.setdefault(msgid, []).append((i, m))

    refusals = []
    if plural:
        refusals.append(f"plural entries (set msgstr[n] by hand): {sorted(plural)}")
    many = sorted(k for k, v in found.items() if len(v) > 1)
    if many:
        refusals.append(f"in more than one entry (contexts): {many}")
    if refusals:
        print("refused, nothing written: " + "; ".join(refusals), file=sys.stderr)
        return 2

    for msgid, [(i, m)] in found.items():
        block = blocks[i]
        head = strip_fuzzy(block[: m.start()].rstrip("\n"))
        id_part = "msgid " + m.group(1)
        blocks[i] = (head + "\n" if head else "") + id_part + "\nmsgstr " + quote(pairs[msgid])

    out = "\n\n".join(blocks)
    if text.endswith("\n") and not out.endswith("\n"):
        out += "\n"
    po_path.write_text(out, encoding="utf-8")
    missing = sorted(set(pairs) - set(found))
    print(f"{po_path.name}: set {len(found)}; missing: {missing}")
    return 1 if missing else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
