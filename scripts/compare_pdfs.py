#!/usr/bin/env python3
"""Compare rsslide PDF output against marp's PDF output for the same .md source.

Compares extracted text only (no layout). Reports per-file similarity and a
PASS/FAIL based on a tolerance threshold.
"""
from __future__ import annotations

import argparse
import difflib
import re
import subprocess
import sys
from pathlib import Path

import pymupdf

REPO = Path(__file__).resolve().parent.parent
MARP_SRC = REPO / "marp"
RSSLIDE_PDFS = REPO / "out" / "generate2"
MARP_PDFS = REPO / "out" / "marp-ref"

# Examples we want to compare (exclude the numbered networking course decks
# since those aren't part of the yaml/marp example parity set).
EXAMPLE_STEMS = [
    "alignment",
    "bullets",
    "bullets-and-code",
    "columns-and-bullets",
    "demo",
    "headings",
    "long-content",
    "svg-diagrams",
    "tables",
    "three-column-bullets",
    "two-column-bullets",
]


def render_with_marp(md: Path, out_pdf: Path) -> None:
    out_pdf.parent.mkdir(parents=True, exist_ok=True)
    subprocess.run(
        ["marp", "--pdf", "--allow-local-files", "-o", str(out_pdf), str(md)],
        check=True,
        cwd=REPO,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )


def normalize(text: str) -> list[str]:
    """Return a list of normalized tokens for comparison.

    Strips leading bullet/number markers, collapses whitespace, lowercases.
    """
    tokens: list[str] = []
    for line in text.splitlines():
        stripped = line.strip()
        if not stripped:
            continue
        # Drop a single leading bullet/number marker.
        stripped = re.sub(r"^[\u2022\u25cf\u25aa\-\*\u2013\u2014]\s*", "", stripped)
        stripped = re.sub(r"^\d+[\.\)]\s+", "", stripped)
        stripped = re.sub(r"\s+", " ", stripped).lower()
        if stripped:
            tokens.append(stripped)
    return tokens


def page_text(pdf: Path) -> list[list[str]]:
    doc = pymupdf.open(pdf)
    return [normalize(page.get_text("text")) for page in doc]


def compare(rsslide_pdf: Path, marp_pdf: Path) -> tuple[float, list[str]]:
    """Return (similarity 0..1, list of notes)."""
    rs_pages = page_text(rsslide_pdf)
    mp_pages = page_text(marp_pdf)
    notes: list[str] = []
    if len(rs_pages) != len(mp_pages):
        notes.append(f"page count: rsslide={len(rs_pages)} marp={len(mp_pages)}")
    n = max(len(rs_pages), len(mp_pages))
    if n == 0:
        return 1.0, notes
    scores: list[float] = []
    for i in range(n):
        rs = rs_pages[i] if i < len(rs_pages) else []
        mp = mp_pages[i] if i < len(mp_pages) else []
        s = difflib.SequenceMatcher(a=rs, b=mp).ratio()
        scores.append(s)
        if s < 1.0:
            missing = [t for t in mp if t not in rs]
            extra = [t for t in rs if t not in mp]
            if missing:
                notes.append(f"  page {i}: missing in rsslide: {missing[:3]}")
            if extra:
                notes.append(f"  page {i}: extra in rsslide: {extra[:3]}")
    return sum(scores) / len(scores), notes


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument(
        "--threshold",
        type=float,
        default=0.80,
        help="minimum per-file similarity to PASS (0..1, default 0.80)",
    )
    ap.add_argument(
        "--no-render",
        action="store_true",
        help="skip marp rendering; reuse existing PDFs in out/marp-ref/",
    )
    ap.add_argument(
        "stems",
        nargs="*",
        help="example stems to compare (default: all examples)",
    )
    args = ap.parse_args()

    stems = args.stems or EXAMPLE_STEMS
    failed = 0
    for stem in stems:
        md = MARP_SRC / f"{stem}.md"
        rs_pdf = RSSLIDE_PDFS / f"{stem}.pdf"
        mp_pdf = MARP_PDFS / f"{stem}.pdf"
        if not md.is_file():
            print(f"SKIP {stem}: {md} missing")
            continue
        if not rs_pdf.is_file():
            print(f"SKIP {stem}: {rs_pdf} missing (run rsconstruct build)")
            continue
        if not args.no_render or not mp_pdf.is_file():
            try:
                render_with_marp(md, mp_pdf)
            except subprocess.CalledProcessError as e:
                print(f"FAIL {stem}: marp render failed ({e})")
                failed += 1
                continue
        score, notes = compare(rs_pdf, mp_pdf)
        verdict = "PASS" if score >= args.threshold else "FAIL"
        if verdict == "FAIL":
            failed += 1
        print(f"{verdict} {stem}: similarity={score:.3f}")
        for note in notes:
            print(note)

    print(f"\n{failed} failed out of {len(stems)} (threshold={args.threshold})")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
