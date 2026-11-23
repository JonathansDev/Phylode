# Phylode feature map

The list of what to verify at runtime. Each file under `Features/` is one surface a person
meets; proof through one entry point does not cover another the file lists.

| Feature | What the person sees |
|---|---|
| [DNS answers](Features/Dns-Answers.md) | Devices get answers, blocked names get `0.0.0.0`, outsiders get nothing |
| [Command line](Features/Command-Line.md) | `allow`, `block`, `status`, `diagnostics`, `--version` and their messages |
| [Weekly summary](Features/Weekly-Summary.md) | `phylode report`: seven numbered sections about the week |
| [Installation](Features/Installation.md) | `install`, `on`, `off`, `uninstall`, `forget`; only ever run in CI |

## Baseline every recipe starts from

- **Build**: the release binary of the commit under test, once per change: the Install test's
  `phylode-release` artifact, or `cargo build --release` when no run fits (SKILL.md 〈Start a
  scratch instance〉). Use the release binary; see the firewall note in [SKILL.md](SKILL.md).
- **Data**: an empty scratch directory `$W` with the configuration from SKILL.md 〈Start a
  scratch instance〉: one list entry (`ads.example.com`), empty allow and block files, and
  the journal in `$W/journal`.
- **Doctor**: SKILL.md 〈Doctor〉 passes before the first action and after anything
  surprising.

## Driving conventions

- Every command takes `--config "$W/c.conf"`; without it, the command reads the installed
  `/usr/local/etc/phylode.conf` and the real journal.
- Queries go to `@::1 -p 15353` (loopback: always served, never rate-limited) unless the
  recipe needs a served or outside source.
- What the service did is read from its files, not inferred from `dig`: queries in
  `$W/journal/<day>.tsv`, events in `$W/journal/events-<day>.tsv`, messages in
  `$W/serve.log`. Days are the Mac's local dates.

## Proof rules

- A missing reply proves nothing until the same path has produced a reply (SKILL.md
  〈Always run the control〉).
- The proof takes the path a person takes: the `phylode` command or a DNS query, never a
  direct edit of a list or journal file, except where a recipe says the file is the input
  (a replaced blocklist).
- Evidence is the command, its output, and the journal or event line it produced, copied
  out of `$W` before cleanup.
- Anything that changes this Mac's system settings (installation, DNS, launchd, the
  firewall) is proved in CI only.
