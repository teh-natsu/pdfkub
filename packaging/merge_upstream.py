"""Helpers for merging upstream PdfKub into PdfKub, during `git merge --no-commit upstream/main`.

  python3 packaging/merge_upstream.py delta OUT.json   PdfKub's own changes (HEAD differs from the
                                                       renamed merge base) and upstream's fate of each
  python3 packaging/merge_upstream.py build OUT.json   write the merged tree: upstream renamed to
                                                       PdfKub, PdfKub's changes merged in three ways
                                                       at each file's new path; prints what needs a hand
  python3 packaging/merge_upstream.py theirs T[=U]...  write upstream's renamed version of files
  python3 packaging/merge_upstream.py ours P[=BASE]... print PdfKub's own diff of files
  python3 packaging/merge_upstream.py resurrected      files PdfKub deleted that the merge brought back
  python3 packaging/merge_upstream.py show FILE...     print conflict hunks (HK_N lines a side)
  python3 packaging/merge_upstream.py res FILE SPEC    resolve hunks in order: o ours, t theirs,
                                                       b ours then theirs, B theirs then ours

Then `git add -A` and `python3 packaging/sync_upstream.py`.
"""
import difflib
import json
import os
import re
import subprocess
import sys
import tempfile

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
os.chdir(ROOT)
sys.path.insert(0, os.path.join(ROOT, "packaging"))
sys.stdout.reconfigure(encoding="utf-8")
import sync_upstream as su  # noqa: E402

HUNK = re.compile(r"<<<<<<< ours\n(.*?)=======\n(.*?)>>>>>>> upstream\n", re.S)


def git(*a):
    return subprocess.run(["git", *a], capture_output=True).stdout


def gtext(*a):
    return git(*a).decode("utf-8", "replace")


def base():
    return gtext("merge-base", "HEAD", "MERGE_HEAD").strip()


def ls(rev):
    return [p for p in gtext("ls-tree", "-r", "--name-only", rev).split("\n") if p]


def rename_path(p):
    if p.startswith(("vendor/", "contributors/")):
        return p
    return "/".join(su.rename_text(x, protect=False) for x in p.split("/"))


def text_rename(path, raw):
    if path.startswith(("vendor/", "contributors/")) or os.path.basename(path) in su.SKIP_FILES or b"\0" in raw:
        return raw
    try:
        return su.rename_text(raw.decode("utf-8")).encode("utf-8")
    except UnicodeDecodeError:
        return raw


def fates(b):
    out = {}
    for line in gtext("diff", "-M30%", "--name-status", b, "MERGE_HEAD").split("\n"):
        parts = line.split("\t")
        if len(parts) < 2:
            continue
        if parts[0].startswith("R"):
            out[parts[1]] = (parts[0], parts[2])
        elif parts[0] in ("M", "D"):
            out[parts[1]] = (parts[0], parts[1])
    return out


def delta(out_path):
    b = base()
    base_by_new = {rename_path(p): p for p in ls(b)}
    fate = fates(b)
    rows = []
    for p in ls("HEAD"):
        if p.startswith(("vendor/", "contributors/")):
            continue
        bp = base_by_new.get(p)
        if bp is None:
            rows.append(("NEW", p, "-", "-"))
            continue
        if text_rename(bp, git("show", f"{b}:{bp}")) == git("show", f"HEAD:{p}"):
            continue
        st, np = fate.get(bp, ("=", bp))
        rows.append((st, p, bp, np))
    json.dump(rows, open(out_path, "w"))
    for r in rows:
        print("\t".join(r))


def build(delta_path):
    b = base()
    rows = json.load(open(delta_path))
    final = {rename_path(f): ("theirs", f) for f in ls("MERGE_HEAD")}
    manual, by_up = [], {}
    for st, p, bp, np in rows:
        if st == "NEW":
            final[p] = ("ours", p)
        elif st == "D":
            manual.append(f"DELETED-UPSTREAM {p}")
        else:
            by_up[rename_path(np)] = (p, bp, np)
    for f in set(ls("HEAD")) | set(ls("MERGE_HEAD")):
        try:
            os.remove(f)
        except FileNotFoundError:
            pass
    tmp = tempfile.mkdtemp()
    for target, (side, src) in sorted(final.items()):
        os.makedirs(os.path.dirname(target) or ".", exist_ok=True)
        if side == "ours" or target == "README.md":
            data = git("show", f"HEAD:{target}")
        elif target in by_up:
            p, bp, np = by_up[target]
            ours_raw = git("show", f"HEAD:{p}")
            theirs_raw = text_rename(np, git("show", f"MERGE_HEAD:{np}"))
            if b"\0" in ours_raw or b"\0" in theirs_raw:
                data = ours_raw
                manual.append(f"BINARY-OURS {target}")
            else:
                names = []
                for n, d in (("ours", ours_raw), ("base", text_rename(bp, git("show", f"{b}:{bp}"))), ("theirs", theirs_raw)):
                    fp = os.path.join(tmp, n)
                    open(fp, "wb").write(d)
                    names.append(fp)
                r = subprocess.run(["git", "merge-file", "-p", "-L", "ours", "-L", "base", "-L", "upstream", *names], capture_output=True)
                data = r.stdout
                if r.returncode:
                    manual.append(f"CONFLICTS {r.returncode:3d} {target}" + (f"  (was {p})" if p != target else ""))
        else:
            data = text_rename(src, git("show", f"MERGE_HEAD:{src}"))
        open(target, "wb").write(data)
    print("\n".join(manual))


def theirs(args):
    for arg in args:
        t, _, u = arg.partition("=")
        u = u or t.replace("pdfkub", "pdfkub")
        open(t, "wb").write(text_rename(u, git("show", f"MERGE_HEAD:{u}")))
        print("theirs", t)


def ours(args):
    b = base()
    for arg in args:
        p, _, bp = arg.partition("=")
        bp = bp or p.replace("pdfkub", "pdfkub")
        a = text_rename(bp, git("show", f"{b}:{bp}")).decode("utf-8", "replace").splitlines(keepends=True)
        h = gtext("show", f"HEAD:{p}").splitlines(keepends=True)
        sys.stdout.writelines(difflib.unified_diff(a, h, "base/" + bp, "ours/" + p, n=2))


def resurrected():
    b = base()
    head = set(ls("HEAD"))
    fate = fates(b)
    for p in ls(b):
        if rename_path(p) in head:
            continue
        st, np = fate.get(p, ("=", p))
        if st != "D" and os.path.exists(rename_path(np)):
            print(f"{st}\t{rename_path(np)}")


def show(paths):
    n = int(os.environ.get("HK_N", "25"))

    def cut(s):
        lines = s.splitlines()
        return "\n".join(lines[:n] + ([f"   … {len(lines) - n} more lines"] if len(lines) > n else []))

    for path in paths:
        s = open(path, encoding="utf-8", newline="").read()
        for i, m in enumerate(HUNK.finditer(s)):
            line = s[: m.start()].count("\n") + 1
            print(f"===== {path}:{line} hunk {i}  (ours {m.group(1).count(chr(10))} / theirs {m.group(2).count(chr(10))} lines)")
            print("\n".join("  | " + c for c in s[: m.start()].splitlines()[-3:]))
            print("<<< ours\n" + cut(m.group(1)) + "\n=== theirs\n" + cut(m.group(2)) + "\n>>>")


def res(path, spec):
    s = open(path, encoding="utf-8", newline="").read()
    assert len(HUNK.findall(s)) == len(spec), (path, len(HUNK.findall(s)), spec)
    for c in spec:
        m = HUNK.search(s)
        o, t = m.group(1), m.group(2)
        s = s[: m.start()] + {"o": o, "t": t, "b": o + t, "B": t + o}[c] + s[m.end():]
    open(path, "w", encoding="utf-8", newline="").write(s)
    print("ok", path, spec)


if __name__ == "__main__":
    cmd, args = sys.argv[1], sys.argv[2:]
    {
        "delta": lambda: delta(args[0]),
        "build": lambda: build(args[0]),
        "theirs": lambda: theirs(args),
        "ours": lambda: ours(args),
        "resurrected": resurrected,
        "show": lambda: show(args),
        "res": lambda: res(args[0], args[1]),
    }[cmd]()
