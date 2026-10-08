#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
"""Rebuild the Polish guess pool from the pinned SJP games dictionary."""

import argparse
import hashlib
import io
from pathlib import Path
import unicodedata
import urllib.request
import zipfile

SOURCE = "https://sjp.pl/sl/growy/sjp-20260901.zip"
SHA256 = "43796ccf34a8ba9b6e965588b842721056b5a89cad4c8c38e057f838d4eaa6a5"
ALPHABET = frozenset("abcdefghijklmnopqrstuvwxyząćęłńóśźż")
ASSETS = Path(__file__).resolve().parents[1] / "late-ssh/assets/le_word/pl"


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--archive", type=Path, help="Use an already downloaded ZIP")
    parser.add_argument("--check", action="store_true", help="Check files without writing")
    args = parser.parse_args()
    if args.archive:
        data = args.archive.read_bytes()
    else:
        with urllib.request.urlopen(SOURCE, timeout=60) as response:
            data = response.read()
    if hashlib.sha256(data).hexdigest() != SHA256:
        parser.error("SJP archive checksum differs from the pinned source")
    with zipfile.ZipFile(io.BytesIO(data)) as archive:
        words = {
            word
            for line in archive.read("slowa.txt").decode("utf-8").splitlines()
            if len(word := unicodedata.normalize("NFC", line.strip().lower())) == 5
            and set(word) <= ALPHABET
        }
        attribution = archive.read("README.txt")
    answer_lines = (ASSETS / "answer_pool.txt").read_text().splitlines()
    answers = set(answer_lines)
    if len(answers) != len(answer_lines) or not answers or not answers <= words:
        parser.error("Curated answers must be unique entries in the source guess pool")
    if any(len(word) != 5 or unicodedata.normalize("NFC", word) != word for word in answers):
        parser.error("Answers must contain exactly five NFC-normalized letters")
    outputs = {
        ASSETS / "valid_extra.txt": ("\n".join(sorted(words - answers)) + "\n").encode(),
        ASSETS / "SJP-README.txt": attribution,
    }
    for path, content in outputs.items():
        if args.check:
            if path.read_bytes() != content:
                parser.error(f"{path.name} differs from the pinned import")
        else:
            path.write_bytes(content)
    print(f"{len(answers)} curated answers; {len(words)} accepted guesses")


if __name__ == "__main__":
    main()
