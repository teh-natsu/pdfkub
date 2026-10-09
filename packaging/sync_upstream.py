"""Bring PdfCraft changes into PdfKub's naming after `git merge upstream/main`.

  python3 packaging/sync_upstream.py

1. Renames PdfCraft to PdfKub in file contents and paths (library crates keep their pdfcraft-*
   names), leaving the credits to upstream alone: "based on PdfCraft", "PdfCraft contributors",
   the UPSTREAM link and the like.
   Then runs `cargo fmt --all`, since the shorter name changes line lengths.
2. Refreshes the sha256 values in ATTRIBUTION.toml, drops entries for files that are gone, and
   regenerates ATTRIBUTION.md (the same output as `cargo xtask assets --write`).
3. Lists ArtCraft branding that came in with the merge (Discord, getartcraft.com, logos, the
   upstream company) and exits with status 1 if there is any: remove it by hand, then run again.

Safe to run more than once.
"""
import hashlib
import os
import re
import subprocess
import sys
import tomllib

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
SKIP_DIRS = {".git", "vendor", "target", "contributors"}
# Upstream's copyright and our own hand-written notices are never rewritten.
SKIP_FILES = {"LICENSE-MIT", "LICENSE-APACHE", "NOTICE", "README.md", "sync_upstream.py"}
def library_crates():
    """Crate folders, plus the planned crates named in xtask's layer table (pdfcraft-arlington…)."""
    names = set(os.listdir(os.path.join(ROOT, "crates")))
    layers = os.path.join(ROOT, "xtask", "src", "layers.rs")
    if os.path.isfile(layers):
        names |= set(re.findall(r'\("([a-z0-9-]+)", Class::(?!Exempt)', open(layers, encoding="utf-8").read()))
    return sorted(names, key=len, reverse=True)


LIB_CRATES = library_crates()
LOWER = re.compile(r"pdfcraft(?![-_](?:" + "|".join(c.replace("-", "[-_]") for c in LIB_CRATES) + "))")

# Credits to upstream that must keep the PdfCraft name, and the pdfcraft prefix shared by the
# library crates (log filters such as `pdfcraft*=info`, `strip_prefix("pdfcraft-")`).
PROTECTED = [
    "github.com/storytold/pdfcraft",
    "Based on PdfCraft",
    "based on PdfCraft",
    "PdfCraft contributors",
    "PdfCraft by the ArtCraft team",
    "pdfcraft*",
    "`pdfcraft-`",
    '"pdfcraft-"',
    '"pdfcraft_"',
]
REPLACEMENTS = [
    ("ai.storyteller.pdfcraft", "io.github.teh_natsu.pdfkub"),
    ("PDFCRAFT", "PDFKUB"),
    ("PdfCraft", "PdfKub"),
    ("Pdfcraft", "Pdfkub"),
]

BRANDING = re.compile(r"discord\.gg|getartcraft|artcraft[-_](mark|logo)|docs/brand|Learning Machines|storyteller\.ai|ai\.storyteller", re.I)
BRANDING_ALLOWED_FILES = {"NOTICE", "README.md", "ROADMAP.md", "LICENSE-MIT", "packaging/sync_upstream.py"}


def rename_text(text, protect=True):
    if protect:
        # A line that names ArtCraft is a credit to upstream ("based on PdfCraft by the ArtCraft
        # team", in any language): leave it as it is.
        return "".join(line if "ArtCraft" in line else rename_text(line, protect=None) for line in text.splitlines(keepends=True))
    masks = {}
    if protect is None:
        protect = True
    if protect:
        for i, phrase in enumerate(PROTECTED):
            token = f"\0KEEP{i}\0"
            masks[token] = phrase
            text = text.replace(phrase, token)
    text = text.replace("storytold/pdfcraft", "teh-natsu/pdfkub")
    for old, new in REPLACEMENTS:
        text = text.replace(old, new)
    text = LOWER.sub("pdfkub", text)
    for token, phrase in masks.items():
        text = text.replace(token, phrase)
    return text


def git(*args):
    return subprocess.run(["git", *args], cwd=ROOT, capture_output=True, text=True, check=True).stdout


def rebrand():
    edited = []
    for dirpath, dirnames, filenames in os.walk(ROOT):
        dirnames[:] = [d for d in dirnames if d not in SKIP_DIRS]
        for name in filenames:
            if name in SKIP_FILES:
                continue
            path = os.path.join(dirpath, name)
            raw = open(path, "rb").read()
            if b"\0" in raw:
                continue
            try:
                text = raw.decode("utf-8")
            except UnicodeDecodeError:
                continue
            new = rename_text(text)
            if new != text:
                open(path, "wb").write(new.encode("utf-8"))
                edited.append(os.path.relpath(path, ROOT).replace(os.sep, "/"))
    moves = {}
    for rel in git("ls-files").split("\n"):
        if not rel or rel.startswith(("vendor/", "contributors/")):
            continue
        parts = rel.split("/")
        for i in range(len(parts)):
            new = rename_text(parts[i], protect=False)
            if new != parts[i]:
                moves["/".join(parts[: i + 1])] = "/".join(parts[:i] + [new])
    for old in sorted(moves, key=lambda p: p.count("/"), reverse=True):
        target = os.path.join(ROOT, moves[old])
        if os.path.isdir(target) and os.path.isdir(os.path.join(ROOT, old)):
            # The folder already exists under the new name: move the files one by one.
            for f in git("ls-files", old).split("\n"):
                if f:
                    os.makedirs(os.path.dirname(os.path.join(ROOT, moves[old] + f[len(old):])), exist_ok=True)
                    git("mv", f, moves[old] + f[len(old):])
        else:
            git("mv", old, moves[old])
    return edited, moves


def render_attribution(m):
    esc = lambda t: t.replace("|", "\\|")
    s = "# Attribution\n\n"
    s += "<!-- Generated from ATTRIBUTION.toml by `cargo xtask assets --write`. Do not edit by hand. -->\n\n"
    s += "Every asset PdfKub includes, bundles or uses to build its published material, with its author, source and licence. "
    s += "The policy is in [AGENTS.md](AGENTS.md) §1. The machine-readable list, with SHA-256 hashes, is [ATTRIBUTION.toml](ATTRIBUTION.toml). "
    s += "Licence texts are kept beside the assets and summarised in [NOTICE](NOTICE).\n\n"
    assets, bundled, fetched, inputs = (m.get(k, []) for k in ("asset", "bundled", "fetched", "build_input"))
    s += f"## In this repository ({len(assets)})\n\n| Asset | Title | Author | Licence | Source | Used for |\n|---|---|---|---|---|---|\n"
    for a in assets:
        s += f"| `{a['path']}` | {esc(a['title'])} | {esc(a['author'])} | {a['licence']} | {esc(a['source'])} | {esc(a['usage'])} |\n"
    s += f"\n## Compiled in through dependencies ({len(bundled)})\n\n| Crate | File | Title | Author | Licence | Source | Used for |\n|---|---|---|---|---|---|---|\n"
    for b in bundled:
        note = " (non-visual data, AGENTS.md §1.1)" if b.get("adobe_data") else ""
        s += f"| `{b['crate']}` {b['version']} | `{b['path']}` | {esc(b['title'])}{note} | {esc(b['author'])} | {b['licence']} | {esc(b['source'])} | {esc(b['usage'])} |\n"
    s += (
        f"\n## Downloaded at build time ({len(fetched)})\n\nFonts are fetched by `cargo xtask demo-pdf` into `target/demo-fonts/`, "
        "OCR models by `cargo xtask models` into `assets/models/`; each is verified by SHA-256 and never committed.\n\n"
        "| File | Title | Author | Licence | Source | Used for |\n|---|---|---|---|---|---|\n"
    )
    for f in fetched:
        s += f"| `{f['file']}` | {esc(f['title'])} | {esc(f['author'])} | {f['licence']} | {esc(f['source'])} | {esc(f['usage'])} |\n"
    s += (
        f"\n## Optional build inputs ({len(inputs)})\n\nNot in this repository and never downloaded by it: compiled in only when the "
        "build sets the option (official releases do). Each input attributes its own files.\n\n"
        "| Input | Option | Title | Author | Licence | Source | Attribution | Used for |\n|---|---|---|---|---|---|---|---|\n"
    )
    for b in inputs:
        s += f"| `{b['name']}` | `{b['option']}` | {esc(b['title'])} | {esc(b['author'])} | {b['licence']} | {esc(b['source'])} | {esc(b['attribution'])} | {esc(b['usage'])} |\n"
    return s


def refresh_attribution():
    path = os.path.join(ROOT, "ATTRIBUTION.toml")
    text = open(path, encoding="utf-8").read()
    starts = [m.start() for m in re.finditer(r"^\[\[", text, re.M)]
    head, blocks = text[: starts[0]], [text[a:b] for a, b in zip(starts, starts[1:] + [len(text)])]
    kept, dropped, rehashed = [], [], 0
    for block in blocks:
        if block.startswith("[[asset]]"):
            rel = re.search(r'^path = "([^"]+)"', block, re.M).group(1)
            file = os.path.join(ROOT, rel)
            if not os.path.isfile(file):
                dropped.append(rel)
                continue
            digest = hashlib.sha256(open(file, "rb").read()).hexdigest()
            new = re.sub(r'^sha256 = "[0-9a-f]*"', f'sha256 = "{digest}"', block, flags=re.M)
            rehashed += new != block
            block = new
        kept.append(block)
    out = head + "".join(kept)
    if not out.endswith("\n"):
        out += "\n"
    open(path, "w", encoding="utf-8", newline="\n").write(out)
    md = render_attribution(tomllib.loads(out))
    open(os.path.join(ROOT, "ATTRIBUTION.md"), "w", encoding="utf-8", newline="\n").write(md)
    return dropped, rehashed


def branding_left():
    found = []
    for rel in git("ls-files").split("\n"):
        if not rel or rel.startswith(("vendor/", "contributors/")) or rel in BRANDING_ALLOWED_FILES:
            continue
        path = os.path.join(ROOT, rel)
        try:
            text = open(path, encoding="utf-8").read()
        except (UnicodeDecodeError, OSError):
            continue
        for n, line in enumerate(text.splitlines(), 1):
            if BRANDING.search(line):
                found.append(f"{rel}:{n}: {line.strip()[:140]}")
    return found


edited, moves = rebrand()
# The new names change line lengths: let rustfmt rewrap (skipped when Rust isn't installed).
try:
    subprocess.run(["cargo", "fmt", "--all"], cwd=ROOT, check=True)
except (OSError, subprocess.CalledProcessError) as e:
    print(f"cargo fmt skipped: {e}")
dropped, rehashed = refresh_attribution()
print(f"renamed text in {len(edited)} files, moved {len(moves)} paths")
for f in edited:
    print(f"  edited  {f}")
for old, new in moves.items():
    print(f"  moved   {old} -> {new}")
print(f"ATTRIBUTION: refreshed {rehashed} hashes, dropped {len(dropped)} entries for missing files")
for f in dropped:
    print(f"  dropped {f}")
left = branding_left()
if left:
    print(f"\nArtCraft branding to remove by hand ({len(left)}):")
    for line in left:
        print(f"  {line}")
    sys.exit(1)
print("no ArtCraft branding left")
