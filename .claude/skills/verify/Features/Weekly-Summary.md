# Weekly summary

What the person actually reads about Phylode: `phylode report` prints the week, in seven
numbered sections, from the query and event journal, and a copy of each finished week is
kept for 12 weeks. Everything important has to be answerable from it without opening the
journal.

## Sub-features

- `W1` Header `Phylode, week of <range>`; with no queries, a line saying none were recorded.
- `W2` Section 1 `Blocked N queries (M total)`, with the devices involved.
- `W3` Section 2 `Signs of devices phoning home`.
- `W4` Section 3 `Phylode's own health`: starts, restarts, list loads and failures,
  upstream outages.
- `W5` Section 4 `Domains you allowed this week`, and section 5 the most-blocked domains.
- `W6` Section 6 `Was there a window with no protection?`, from `blocklist-failed` and
  restarts.
- `W7` Section 7 `Were any records dropped?`, saying "cannot tell" when journal files are
  missing.
- `W8` `phylode report <days>` covers another span; the journal level (`All`, `Domains`,
  `Zero`) makes sections say "cannot tell" for what is no longer kept.

## How to get to it

- `phylode report` (7 days) or `phylode report <days>` in Terminal or over SSH.
- The saved weeks in the journal's `weekly/` folder.

## Driving it

Preconditions: the baseline in [Features.md](../Features.md), and some traffic: run the
`A1`, `A2` and `C1` steps from [DNS answers](Dns-Answers.md) and
[Command line](Command-Line.md) first, then stop the instance so the journal is flushed.

| Action | Command | Observe |
|---|---|---|
| `W1`–`W7` the week | `phylode --config "$W/c.conf" report` | the header and seven sections; section 1 counts the `ads.example.com` queries; section 3 says started once; section 4 lists `example.org`; section 6 says protection was continuous |
| `W6` a gap | restart with `$W/list.txt` unreadable, query once, restore the list, then `report` | section 6 names the window |
| `W7` missing files | delete one day's `events-<day>.tsv` in `$W/journal`, then `report` | section 7 says it cannot tell |
| `W8` less kept | add `journal = Zero`, restart, query, `report` | counts only; the domain sections say they cannot tell |

## Gotchas

- The journal is written in batches by its own thread; stop the instance (or wait for the
  next flush) before reading `report`, or the latest queries are missing.
- A run near local midnight writes into two days' files.
- An empty week is a valid result (`W1`), not a failure; seed traffic first.
