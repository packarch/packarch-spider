#!/usr/bin/env python3
"""
Generate po/<lang>/packarch-spider-extra.po.

In KDE's KPatience, which Packarch Spider is based on, a few user-visible strings (standard menu titles,
Quit, Undo, Show Menubar, ...) were translated by the KDE Frameworks and
libkdegames catalogs, not by kpat.po. Now that the game does not use those
libraries any more, this script copies the needed translations into small
"packarch-spider-extra" catalogs:

  1. extract every translatable string of src/ with xgettext;
  2. keep the ones that packarch-spider.po does not provide;
  3. look them up (by msgid, ignoring the context, and ignoring the
     keyboard accelerator '&' if needed) in the catalogs shipped with the
     official KDE tarballs, which are downloaded automatically;
  4. write po/<lang>/packarch-spider-extra.po for every language of po/.

Usage: tools/update-extra-translations.py [--cache DIR]

SPDX-License-Identifier: GPL-2.0-or-later
"""

import argparse
import os
import re
import subprocess
import sys
import tarfile
import tempfile
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

KDE_TARBALLS = [
    "https://download.kde.org/stable/release-service/26.08.3/src/libkdegames-26.08.3.tar.xz",
    "https://download.kde.org/stable/frameworks/6.31/kxmlgui-6.31.0.tar.xz",
    "https://download.kde.org/stable/frameworks/6.31/kconfigwidgets-6.31.0.tar.xz",
    "https://download.kde.org/stable/frameworks/6.31/kwidgetsaddons-6.31.0.tar.xz",
    "https://download.kde.org/stable/frameworks/6.31/kconfig-6.31.0.tar.xz",
    "https://download.kde.org/stable/frameworks/6.31/kcoreaddons-6.31.0.tar.xz",
]

# Strings that no KDE catalog provides: hand-made translations.
MANUAL = {
    ("@action", "About &Qt"): {
        "fr": "À propos de &Qt",
        "de": "Über &Qt",
        "es": "Acerca de &Qt",
        "it": "Informazioni su &Qt",
        "pt": "Acerca do &Qt",
        "pt_BR": "Sobre o &Qt",
        "nl": "Info over &Qt",
        "ca": "Quant a &Qt",
    },
    ("@title", "Authors"): {
        "fr": "Auteurs",
    },
    # Strings specific to Packarch Spider
    (None, "Patience card games"): {
        "fr": "Jeux de patience",
    },
    (None, "Based on %1, the patience game of the KDE project."): {
        "fr": "Basé sur %1, le jeu de patience du projet KDE.",
    },
    ("@item:inlistbox file type", "%1 saved game"): {
        "fr": "Partie enregistrée de %1",
    },
    ("@item:inlistbox file type", "%1 saved game, legacy format"): {
        "fr": "Partie enregistrée de %1, ancien format",
    },
    ("@item:inlistbox file type", "KPatience saved game"): {
        "fr": "Partie enregistrée de KPatience",
    },
    ("@item:inlistbox file type", "All files"): {
        "fr": "Tous les fichiers",
    },
}

XGETTEXT_KEYWORDS = [
    "-ki18n:1", "-ki18nc:1c,2", "-ki18np:1,2", "-ki18ncp:1c,2,3",
    "-kki18n:1", "-kki18nc:1c,2", "-kkli18n:1", "-kkli18nc:1c,2",
    "-ktr2i18n:1",
]

# ---------------------------------------------------------------- PO parsing


def _unquote_lines(lines):
    return "".join(re.findall(r'"(.*)"', "\n".join(lines)))


def parse_po(path):
    """Returns a list of dicts: ctx, id, id_plural, strs (list), fuzzy, obsolete.
    Strings are kept escaped exactly as in the file."""
    entries = []
    text = Path(path).read_text(encoding="utf-8", errors="replace")
    for block in re.split(r"\n\s*\n", text):
        lines = block.strip("\n").split("\n")
        obsolete = all(l.startswith("#~") or not l.strip() for l in lines if not l.startswith("#,") and not l.startswith("#:") and not l.startswith("# "))
        cur = None
        fields = {}
        fuzzy = False
        for line in lines:
            if line.startswith("#,") and "fuzzy" in line:
                fuzzy = True
            if line.startswith("#~"):
                line = line[2:].lstrip()
                if line.startswith("|"):
                    continue
            elif line.startswith("#"):
                continue
            m = re.match(r'^(msgctxt|msgid_plural|msgid|msgstr(?:\[\d+\])?)\s+(".*")\s*$', line)
            if m:
                cur = m.group(1)
                fields[cur] = [m.group(2)]
            elif line.startswith('"') and cur:
                fields[cur].append(line)
        if "msgid" not in fields:
            continue
        strs = []
        if "msgstr" in fields:
            strs = [_unquote_lines(fields["msgstr"])]
        else:
            i = 0
            while f"msgstr[{i}]" in fields:
                strs.append(_unquote_lines(fields[f"msgstr[{i}]"]))
                i += 1
        entries.append({
            "ctx": _unquote_lines(fields["msgctxt"]) if "msgctxt" in fields else None,
            "id": _unquote_lines(fields["msgid"]),
            "id_plural": _unquote_lines(fields["msgid_plural"]) if "msgid_plural" in fields else None,
            "strs": strs,
            "fuzzy": fuzzy,
            "obsolete": obsolete,
        })
    return entries


def header_plural_forms(po_path):
    for e in parse_po(po_path):
        if e["id"] == "" and e["strs"]:
            m = re.search(r"Plural-Forms:([^\\]*)\\n", e["strs"][0])
            if m:
                return m.group(1).strip()
    return None


# ---------------------------------------------------------------- helpers


def strip_accel(s):
    # "作者(&U)" -> "作者", "A&uthors" -> "Authors" (but keep "&&")
    s = re.sub(r"\(&[^)]\)", "", s)
    return re.sub(r"&(?!&)", "", s)


def download_catalogs(cache):
    cache.mkdir(parents=True, exist_ok=True)
    roots = []
    for url in KDE_TARBALLS:
        name = url.rsplit("/", 1)[1]
        archive = cache / name
        if not archive.exists():
            print(f"Downloading {url}", file=sys.stderr)
            urllib.request.urlretrieve(url, archive)
        dest = cache / name.replace(".tar.xz", "")
        if not dest.exists():
            with tarfile.open(archive) as tar:
                members = [m for m in tar.getmembers() if re.search(r"/(po|poqm)/[^/]+/[^/]+\.po$", m.name)]
                tar.extractall(cache, members=members, filter="data")
        roots.append(dest)
    return roots


def code_strings():
    sources = sorted(str(p) for p in (ROOT / "src").rglob("*.cpp"))
    with tempfile.NamedTemporaryFile(suffix=".pot") as pot:
        subprocess.run(["xgettext", "-C", "--from-code=UTF-8", *XGETTEXT_KEYWORDS, "-o", pot.name, *sources], check=True)
        return [e for e in parse_po(pot.name) if e["id"]]


def po_escape_header(plural_forms):
    h = ('Project-Id-Version: packarch-spider-extra\\n'
         'MIME-Version: 1.0\\n'
         'Content-Type: text/plain; charset=UTF-8\\n'
         'Content-Transfer-Encoding: 8bit\\n')
    if plural_forms:
        h += f"Plural-Forms: {plural_forms}\\n"
    return h


# ---------------------------------------------------------------- main


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--cache", type=Path, default=Path(tempfile.gettempdir()) / "kpat-kde-catalogs",
                        help="where the KDE tarballs are downloaded (default: %(default)s)")
    args = parser.parse_args()

    roots = download_catalogs(args.cache)
    wanted = code_strings()

    langs = sorted(p.parent.name for p in (ROOT / "po").glob("*/packarch-spider.po"))
    total = 0
    for lang in langs:
        kpat_po = ROOT / "po" / lang / "packarch-spider.po"
        known = {(e["ctx"], e["id"]) for e in parse_po(kpat_po) if not e["obsolete"]}
        missing = [e for e in wanted if (e["ctx"], e["id"]) not in known]

        # Index the KDE catalogs of this language: exact msgid, then accelerator-less.
        exact, loose = {}, {}
        for root in roots:
            for po in sorted((root / "po" / lang).glob("*.po")) + sorted((root / "poqm" / lang).glob("*.po")):
                for e in parse_po(po):
                    if not e["id"] or e["fuzzy"] or not e["strs"] or not e["strs"][0]:
                        continue
                    rank = 1 if e["obsolete"] else 0
                    key = e["id"]
                    if key not in exact or exact[key][0] > rank:
                        exact[key] = (rank, e)
                    lkey = strip_accel(key)
                    if lkey not in loose or loose[lkey][0] > rank:
                        loose[lkey] = (rank, e)

        out = []
        for e in missing:
            strs = None
            manual = MANUAL.get((e["ctx"], e["id"]), {}).get(lang)
            if manual:
                strs = [manual]
            elif e["id"] in exact and bool(exact[e["id"]][1]["id_plural"]) == bool(e["id_plural"]):
                strs = exact[e["id"]][1]["strs"]
            elif strip_accel(e["id"]) in loose:
                src = loose[strip_accel(e["id"])][1]
                if bool(src["id_plural"]) == bool(e["id_plural"]):
                    strs = src["strs"]
                    if "&" not in e["id"]:
                        strs = [strip_accel(s) for s in strs]
            if strs:
                out.append((e, strs))

        target = ROOT / "po" / lang / "packarch-spider-extra.po"
        if not out:
            if target.exists():
                target.unlink()
            continue

        lines = [
            "# Translations of standard strings that KPatience (which Packarch Spider",
            "# is based on) used to get from",
            "# KDE Frameworks / libkdegames. Generated by tools/update-extra-translations.py",
            "# from the KDE catalogs; the copyright of the translations belongs to",
            "# their respective KDE translators. Do not edit by hand.",
            "#",
            "# SPDX-License-Identifier: LGPL-2.0-or-later",
            'msgid ""',
            'msgstr ""',
        ]
        lines += [f'"{part}\\n"' for part in po_escape_header(header_plural_forms(kpat_po)).split("\\n") if part]
        for e, strs in out:
            lines.append("")
            if e["ctx"] is not None:
                lines.append(f'msgctxt "{e["ctx"]}"')
            lines.append(f'msgid "{e["id"]}"')
            if e["id_plural"]:
                lines.append(f'msgid_plural "{e["id_plural"]}"')
                for i, s in enumerate(strs):
                    lines.append(f'msgstr[{i}] "{s}"')
            else:
                lines.append(f'msgstr "{strs[0]}"')
        target.write_text("\n".join(lines) + "\n", encoding="utf-8")
        total += len(out)
        print(f"{lang}: {len(out)}/{len(missing)} strings", file=sys.stderr)

    print(f"Done: {total} translations written.", file=sys.stderr)


if __name__ == "__main__":
    main()
