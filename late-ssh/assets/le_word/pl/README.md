# Polish Le Word dictionaries

`answer_pool.txt` contains 704 curated daily answers. `valid_extra.txt` contains
27,997 additional accepted guesses; the runtime also accepts every answer.
Both files contain lowercase, NFC-normalized words of exactly five letters.
Polish diacritics are retained and count as separate letters.

## Source and attribution

The guess dictionary is derived from **Słownik SJP.PL — wersja do gier słownych**,
[SJP.PL's games dictionary](https://sjp.pl/sl/growy/), snapshot
[`sjp-20260901.zip`](https://sjp.pl/sl/growy/sjp-20260901.zip).
Its SHA-256 is
`43796ccf34a8ba9b6e965588b842721056b5a89cad4c8c38e057f838d4eaa6a5`.

We use the source under
[Creative Commons Attribution 4.0 International](https://creativecommons.org/licenses/by/4.0/).
The archive's original attribution and licensing notice is preserved in
`SJP-README.txt`. Changes: lowercase/NFC normalization, restriction to the
supported alphabet and five letters, deduplication, and separation of curated
answers from the remaining guesses. SJP.PL does not endorse this game.

## Answer policy

Daily answers are familiar Polish vocabulary and common inflections, rather
than every five-letter entry in SJP. A manually assembled candidate list was
screened using Polish Zipf frequency >= 3.5 from `wordfreq` 3.1.1 and reviewed to
remove obscure forms and entries whose frequency was inflated by proper names.
Frequency is a screening aid, not an automatic rule for future additions.
The checked-in answer file is the editorial source of truth.

`translations.tsv` records 162 useful equivalents from the existing English
answer dictionary, including common inflections and approximate equivalents.
For example, `glass → szkło`, `heart → serce`, `bread → chleb` and
`horse → konie`. English words without a familiar five-letter equivalent are
replaced by independent Polish candidates; words are not shortened or padded.
These mappings do not pair or schedule the daily answers.

The guess pool deliberately accepts obscure vocabulary, inflections and the
other words permitted by the SJP games dictionary. Inclusion there does not
make a word eligible to be a daily answer.

## Rebuilding the guess pool

From the repository root:

```sh
uv run scripts/update-le-word-polish-guesses.py
uv run scripts/update-le-word-polish-guesses.py --check
```

Use `--archive /path/to/sjp-20260901.zip` for an offline rebuild. The importer
checks the pinned checksum and requires every curated answer to appear in
the source. It preserves `answer_pool.txt` and `translations.tsv`; changes to
those files require editorial review. Run the Le Word dictionary tests after
editing either pool. A new SJP snapshot requires updating the pinned URL,
checksum and this attribution together.
