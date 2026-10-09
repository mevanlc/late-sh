# Polish Le Word dictionaries

`answer_pool.txt` contains 1,049 curated daily answers. `valid_extra.txt` contains
27,652 additional accepted guesses; the runtime also accepts every answer,
for a total of 28,701 accepted guesses.
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

Daily answers are familiar Polish vocabulary and common inflections. The original
704-word seed came from a manually assembled candidate list screened using Polish
Zipf frequency >= 3.5 from [`wordfreq` 3.1.1](https://github.com/rspeer/wordfreq),
then reviewed to remove obscure forms and entries whose frequency was inflated
by proper names.

The 2026-10-09 expansion adds 345 words selected after ranking the entire remaining
guess pool by that frequency data. It favours ordinary nouns and dictionary
forms, with a limited selection of familiar verb forms, adverbs, function words
and numerals. Proper names, abbreviations, archaic forms and specialist vocabulary
were excluded from this batch. Existing answers were retained.

[`answer_reviews.tsv`](answer_reviews.tsv) records each addition's English gloss,
category and selection basis. `frequency-reviewed` marks the 274 selections at
or above 3.5; `familiarity-reviewed` marks 71 familiar words below it, such as
`łyżka` (spoon), `banan` (banana), `motyl` (butterfly) and `cegła` (brick).
Frequency ranks candidates for review; it does not automatically admit or reject
a word. Short English glosses are editorial aids, not gameplay translations or
paired daily answers. The checked-in answer file is the editorial source of truth.

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
the source. It preserves `answer_pool.txt`, `translations.tsv` and
`answer_reviews.tsv`; changes to those files require editorial review. When
promoting an existing accepted guess, add it to the answer file and record its
meaning and selection basis in `answer_reviews.tsv`, then rebuild the guess pool
to remove the overlap. Promotion preserves the combined accepted-guess set.
Run the Le Word dictionary tests after editing either pool. A new SJP snapshot
requires updating the pinned URL, checksum and this attribution together.
