#!/usr/bin/env python3
"""Generate .SRCINFO from a simple PKGBUILD without makepkg (macOS release box).

Handles the flat PKGBUILD shape used in packaging/aur/*: scalar assignments,
one-line or multi-line bash arrays, and ${pkgver} substitution. Run makepkg
--printsrcinfo on Arch when in doubt; the output must match.
"""
import re, sys, pathlib

def parse(text):
    vals = {}
    text = "\n".join(line.split(" #")[0].rstrip() if not line.lstrip().startswith("#") else "" for line in text.splitlines())
    for m in re.finditer(r"^(\w+)=(\(.*?\)|\"[^\"]*\"|'[^']*'|\S+)$", text, re.M | re.S):
        key, raw = m.group(1), m.group(2).strip()
        if raw.startswith("("):
            items = re.findall(r"'([^']*)'|\"([^\"]*)\"", raw[1:-1])
            vals[key] = [a or b for a, b in items]
        else:
            vals[key] = raw.strip("'\"")
    def sub(s):
        return re.sub(r"\$\{(\w+)\}", lambda mm: str(vals.get(mm.group(1), "")), s)
    return {k: ([sub(x) for x in v] if isinstance(v, list) else sub(v)) for k, v in vals.items()}

def srcinfo(v):
    out = [f"pkgbase = {v['pkgname']}"]
    for key in ["pkgdesc", "pkgver", "pkgrel", "url"]:
        if key in v: out.append(f"\t{key} = {v[key]}")
    for key in ["arch", "license", "makedepends", "depends", "optdepends", "provides", "conflicts", "source", "sha256sums"]:
        for item in v.get(key, []):
            out.append(f"\t{key} = {item}")
    out.append("")
    out.append(f"pkgname = {v['pkgname']}")
    return "\n".join(out) + "\n"

if __name__ == "__main__":
    path = pathlib.Path(sys.argv[1])
    sys.stdout.write(srcinfo(parse(path.read_text())))
