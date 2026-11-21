#!/usr/bin/env python3
"""Check that the names the docs give in backticks still exist in the code.

Scans AGENTS.md, doc/*.md and docs/*.md (fenced code blocks are skipped) and
checks every inline code span that is one bare token:

  paths      `src/app/theme.rs`, `doc/`, `shell/playback/` (a partial path
             passes when a tracked file or folder ends with it), bare file
             names such as `widget.rs` (some tracked file has that name)
  actions    `win.show-queue`, `app.view_artist`: registered in the code
  classes    `.page-bar--solid`, `canto-cards`: found in src/ or data/
  icons      `canto-lyrics-symbolic`: found in src/ or data/
  symbols    `ThemeKind::Omarchy`, `apply_rail`, `BAR_TIER_WIDTHS`,
             `PageHeader::set_floating(true)`: every name found in src/,
             crates/ or data/ (CamelCase with two humps, snake_case,
             SCREAMING_CASE, `::` paths, calls)

A span written right after "planned:" is skipped (a path that does not exist
yet). Plain words, commands with spaces, numbers, keys and placeholders
(`<name>`) are not checked.

It also checks the code itself: every `win.*` and `app.*` action name used in
a .rs or .blp file under src/ is registered (a string literal with that name
in a source file that adds or installs actions).

Usage: check-doc-refs.py [--root DIR]   exits 1 on any unknown name.
Needs only Python 3. In a git checkout it reads tracked and new (not ignored)
files; elsewhere every file outside the build folders.
"""

import argparse
import os
import re
import subprocess
import sys

# Paths that are generated or local, never tracked, but fine to name.
UNTRACKED_OK = {
    "build",
    "build/",
    "target",
    "target/",
    "cargo",
    "src/config.rs",
    "build/src/canto",
    "build/src/canto.gresource",
    "src/canto.gresource",
}

# Names from GTK, GLib, gettext or a system tool that the docs name on purpose
# (a trap with that API) and the code does not have to use.
EXTERNAL = {
    "set_lines",
    "css_name",
    "msgid_plural",
    "dbus-daemon",
}

SKIP_DIRS = {".git", "build", "target", "cargo", "builddir", ".flatpak-builder", "__pycache__"}

CODE_ROOTS = ("src/", "crates/", "data/")
# Kebab-case names (crates, keys, tools) may also live in the build files and scripts.
KEBAB_ROOTS = CODE_ROOTS + ("scripts/", "flatpak/", ".github/", "po/POTFILES", "Cargo.toml", "meson.build", "Justfile")

SOURCE_EXTS = (".rs", ".blp", ".css", ".xml", ".ui", ".in", ".toml", ".json", ".svg", ".desktop", ".sh", ".py", ".yml", ".build")

FILE_EXTS = (".rs", ".blp", ".css", ".md", ".sh", ".py", ".xml", ".toml", ".json", ".po", ".pot", ".svg", ".yml", ".build", ".in", ".lock")

IDENT = r"[A-Za-z_][A-Za-z0-9_]*"
RE_ACTION = re.compile(r"^(win|app)\.([a-z0-9][a-z0-9_-]*)$")
RE_ICON = re.compile(r"^[a-z0-9][a-z0-9{},_-]*-symbolic$")
RE_CLASS = re.compile(r"^\.([a-z][a-z0-9_-]*)$")
RE_KEBAB = re.compile(r"^[a-z][a-z0-9]*(-[a-z0-9]+)+(--[a-z0-9-]+)?$")
RE_CAMEL = re.compile(r"^[A-Z][a-z0-9]+[A-Z][A-Za-z0-9]*$")
RE_SNAKE = re.compile(r"^[A-Za-z][A-Za-z0-9]*(_[A-Za-z0-9]+|_\*)+$")
RE_RUST_PATH = re.compile(r"^(" + IDENT + r")(::(" + IDENT + r"|\*))+$")
RE_CALL = re.compile(r"^(" + IDENT + r"(::" + IDENT + r")*)\(.*\)$")
RE_BLP_TYPE = re.compile(r"^(Adw|Gtk|Gio)\.([A-Z][A-Za-z0-9]+)$")
RE_PATHLIKE = re.compile(r"^[A-Za-z0-9_.{},*-]+(/[A-Za-z0-9_.{},*-]*)+$")
RE_ACTION_USE = re.compile(r"\b(win|app)\.([a-z0-9][a-z0-9_-]*)")
RE_STRING = re.compile(r'"([a-z0-9][a-z0-9_-]*)"')


def tracked_files(root):
    try:
        out = subprocess.run(
            ["git", "-C", root, "ls-files", "-z", "--cached", "--others", "--exclude-standard"],
            check=True,
            capture_output=True,
        ).stdout.decode()
        files = [f for f in out.split("\0") if f and os.path.exists(os.path.join(root, f))]
        if files:
            return files
    except (OSError, subprocess.CalledProcessError):
        pass
    files = []
    for top, dirs, names in os.walk(root):
        dirs[:] = [d for d in dirs if d not in SKIP_DIRS]
        for n in names:
            files.append(os.path.relpath(os.path.join(top, n), root))
    return files


def expand_braces(text):
    m = re.search(r"\{([^{}]*)\}", text)
    if not m:
        return [text]
    head, tail = text[: m.start()], text[m.end() :]
    return [r for opt in m.group(1).split(",") for r in expand_braces(head + opt + tail)]


def glob_regex(text):
    return re.compile("^" + ".*".join(re.escape(p) for p in text.split("*")) + "$")


class Corpus:
    def __init__(self, root):
        self.root = root
        self.files = sorted(tracked_files(root))
        self.file_set = set(self.files)
        self.dir_set = set()
        for f in self.files:
            parts = f.split("/")
            for i in range(1, len(parts)):
                self.dir_set.add("/".join(parts[:i]))
        self.basenames = {os.path.basename(f) for f in self.files}
        self.words = set()
        self.kebab = set()
        self.texts = {}
        for f in self.files:
            in_code = f.startswith(CODE_ROOTS)
            in_kebab = f.startswith(KEBAB_ROOTS) or f in KEBAB_ROOTS
            if not (in_code or in_kebab) or not f.endswith(SOURCE_EXTS + ("POTFILES", "Justfile")):
                continue
            try:
                with open(os.path.join(root, f), encoding="utf-8", errors="replace") as fh:
                    text = fh.read()
            except OSError:
                continue
            self.texts[f] = text
            if in_code:
                self.words.update(re.findall(r"[A-Za-z0-9_]+", text))
            self.kebab.update(re.findall(r"[A-Za-z0-9_][A-Za-z0-9_-]*", text))
        self.kebab.update(self.words)

    def has_word(self, word):
        if "*" in word:
            rx = glob_regex(word)
            return any(rx.match(w) for w in self.words)
        return word in self.words

    def has_kebab(self, word):
        if "*" in word:
            rx = glob_regex(word)
            return any(rx.match(w) for w in self.kebab)
        return word in self.kebab

    def has_path(self, path):
        if path in UNTRACKED_OK:
            return True
        if "*" in path:
            rx = glob_regex(path.rstrip("/"))
            return any(rx.match(f) for f in self.files) or any(rx.match(d) for d in self.dir_set)
        want_dir = path.endswith("/")
        p = path.rstrip("/")
        if p.startswith("./"):
            p = p[2:]
        if want_dir:
            return p in self.dir_set or any(d.endswith("/" + p) for d in self.dir_set)
        if p in self.file_set or p in self.dir_set:
            return True
        return any(f.endswith("/" + p) for f in self.files) or any(
            d.endswith("/" + p) for d in self.dir_set
        )

    def registered_actions(self):
        names = set()
        for f, text in self.texts.items():
            if not (f.startswith("src/") and f.endswith(".rs")):
                continue
            if "/gtk_tests/" in f or f.endswith("_tests.rs"):
                continue
            if "add_action" not in text and "install_action" not in text:
                continue
            names.update(RE_STRING.findall(text))
            # install_action("card.activate", ...) registers "activate" in group "card"
            names.update(re.findall(r'install_action\(\s*"[a-z0-9_-]+\.([a-z0-9_-]+)"', text))
        return names


def doc_files(corpus):
    out = []
    for f in corpus.files:
        if not f.endswith(".md"):
            continue
        if f == "AGENTS.md" or (f.startswith(("doc/", "docs/")) and f.count("/") == 1):
            out.append(f)
    return out


def code_spans(text):
    """(line number, span, text just before it) for every inline code span."""
    lines = text.split("\n")
    kept = []
    fence = None
    for line in lines:
        s = line.lstrip()
        if fence:
            kept.append("")
            if s.startswith(fence):
                fence = None
            continue
        if s.startswith("```") or s.startswith("~~~"):
            fence = s[:3]
            kept.append("")
            continue
        kept.append(line)
    body = "\n".join(kept)
    for m in re.finditer(r"(`+)(.+?)\1", body, re.S):
        span = m.group(2).strip().replace("\n", " ")
        line_no = body.count("\n", 0, m.start()) + 1
        before = body[max(0, m.start() - 12) : m.start()]
        yield line_no, span, before


def check_token(tok, corpus, actions):
    """None when not checked, "" when found, otherwise a short reason."""
    if not tok or " " in tok or any(c in tok for c in "<>\"'=$~@#+|\\"):
        return None
    if tok.startswith(("http", "-", "/", "!", "[")) or tok.startswith("github.com"):
        return None
    if re.match(r"^[0-9]", tok) or not re.search(r"[A-Za-z]", tok) or tok in EXTERNAL:
        return None
    if tok.startswith(".") and (tok in corpus.file_set or tok in corpus.basenames):
        return ""
    # GTK's C type names (`GtkPicture`, `AdwToolbarView`) are gtk::Picture in Rust
    m = re.match(r"^(Gtk|Adw)([A-Z][A-Za-z0-9]+)$", tok)
    if m:
        return "" if corpus.has_word(m.group(2)) else "widget type not found"

    m = RE_ACTION.match(tok)
    if m:
        return "" if m.group(2) in actions else "action not registered"

    if RE_ICON.match(tok):
        missing = [i for i in expand_braces(tok) if not corpus.has_kebab(i)]
        return "icon not found: " + ", ".join(missing) if missing else ""

    m = RE_CLASS.match(tok)
    if m:
        return "" if corpus.has_kebab(m.group(1)) else "CSS class not found"

    if "/" in tok and "::" not in tok and RE_PATHLIKE.match(tok):
        missing = [p for p in expand_braces(tok) if not corpus.has_path(p)]
        return "path not found: " + ", ".join(missing) if missing else ""

    if tok.endswith(FILE_EXTS) and re.match(r"^[A-Za-z0-9_.{},-]+$", tok) and "." in tok:
        missing = [p for p in expand_braces(tok) if p not in corpus.basenames and p not in UNTRACKED_OK]
        return "file not found: " + ", ".join(missing) if missing else ""

    m = RE_BLP_TYPE.match(tok)
    if m:
        return "" if corpus.has_word(m.group(2)) else "widget type not found"

    m = RE_CALL.match(tok)
    if m:
        tok = m.group(1)
    if RE_RUST_PATH.match(tok) or (m and re.match("^" + IDENT + "$", tok)):
        # extension traits (`ButtonExt`) come from the gtk prelude, unnamed in the code
        segs = [s for s in tok.split("::") if s != "*" and not re.match(r"^[A-Z][A-Za-z0-9]*Ext$", s)]
        missing = [s for s in segs if not corpus.has_word(s)]
        return "name not found: " + ", ".join(missing) if missing else ""

    if RE_CAMEL.match(tok):
        return "" if corpus.has_word(tok) else "name not found"
    if RE_SNAKE.match(tok):
        # env settings such as `CANTO_JOBS` live in the scripts
        return "" if corpus.has_word(tok) or corpus.has_kebab(tok) else "name not found"

    if RE_KEBAB.match(tok):
        missing = [k for k in expand_braces(tok) if not corpus.has_kebab(k)]
        return "name not found: " + ", ".join(missing) if missing else ""
    if "{" in tok and re.match(r"^[a-z0-9{},-]+$", tok):
        missing = [k for k in expand_braces(tok) if not corpus.has_kebab(k)]
        return "name not found: " + ", ".join(missing) if missing else ""

    return None


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--root", default=os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..")))
    args = ap.parse_args()

    corpus = Corpus(args.root)
    actions = corpus.registered_actions()
    problems = []
    checked = 0

    for doc in doc_files(corpus):
        with open(os.path.join(args.root, doc), encoding="utf-8") as fh:
            text = fh.read()
        for line_no, span, before in code_spans(text):
            if "planned:" in before:
                continue
            tok = span.rstrip(",;")
            reason = check_token(tok, corpus, actions)
            if reason is not None:
                checked += 1
            if reason:
                problems.append(f"{doc}:{line_no}: `{tok}`: {reason}")

    used = 0
    for f, text in sorted(corpus.texts.items()):
        if not f.startswith("src/") or not f.endswith((".rs", ".blp")):
            continue
        for i, line in enumerate(text.split("\n"), 1):
            for m in RE_ACTION_USE.finditer(line):
                prefix = line[: m.start()]
                # only action names in strings: "win.x" or "action(win.x)"
                if not re.search(r'("|action\()$', prefix):
                    continue
                used += 1
                if m.group(2) not in actions:
                    problems.append(f"{f}:{i}: {m.group(1)}.{m.group(2)}: action used but never registered")

    for p in problems:
        print("FAIL: " + p)
    if problems:
        print(f"{len(problems)} unknown name(s). Fix the doc or the code; a path that does not exist yet goes after \"planned:\".")
        return 1
    print(f"Doc references OK: {checked} names in {len(doc_files(corpus))} docs, {used} action uses in src/.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
