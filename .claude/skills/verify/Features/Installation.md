# Installation

How Phylode gets onto a Mac and off it again: `install` sets up the boot service and
downloads the lists, `on` points this Mac's DNS at it, `off` puts the DNS back, `uninstall`
removes the service while keeping the person's files, and `forget` deletes the journal now.
All of them change system settings, so they are driven only on a throwaway CI runner, never
on the Mac running this session.

## Sub-features

- `I1` `sudo phylode install` installs or updates the service, waits until it answers, and
  stops without changing anything when an install folder is writable by a standard account.
- `I2` `sudo phylode on` points this Mac's DNS at Phylode and prints the Mac's address;
  `sudo phylode off` restores the DNS exactly as it was, spaces in service names included.
- `I3` `sudo phylode uninstall` removes the service and keeps the configuration, lists and
  journal.
- `I4` `sudo phylode forget` deletes the journal and the saved weeks while the service runs.
- `I5` Updating by the README's steps replaces the running copy without a gap.
- `I6` The daily list update at 04:17 (`phylode run lists`) swaps in a complete new list and
  sets aside one with under half the entries.
- `I7` `brew install JonathansDev/tap/phylode`, then the `sudo /opt/homebrew/opt/phylode/bin/phylode
  install` line Homebrew prints, installs three launchd jobs that all run
  `/usr/local/bin/phylode`; `phylode` with or without `sudo` is that copy. A build from source
  installs two (no upgrade helper).
- `I8` After `brew upgrade`, the upgrade helper (`phylode run upgrade`, root) installs the new
  version once its copy matches the published checksum; a copy that does not match is
  refused and remembered.
- `I9` A new version that does not answer within 15 seconds, or stops within 60, is replaced by
  the version before it, through `install` (printed) and through an upgrade (written to the
  weekly summary's section 3, and the version is not tried again).

## How to get to it

- Terminal on the Mac, with `sudo`, from the folder built per the README.
- The daily update runs from launchd (`deploy/`).

## Driving it

Preconditions: a branch pushed to GitHub. Every row runs in CI; read the run log.

| Action | Command | Observe |
|---|---|---|
| `I1`–`I4` | open a pull request, or run `install.yml` by its manual trigger on the branch | each step green in the Install test; the refusal step prints `Did not finish: ... Nothing was installed` |
| `I5` | the Upgrade test (`upgrade.yml`) on the pull request | it installs the previous release and updates by the README's steps |
| `I6` | `./target/release/phylode run lists "$W" "$W/c.conf"` locally downloads the real lists into a scratch folder; run an edited copy of `deploy/update-blocklists.sh` with `bash` to test its limits | the new list in `$W`, or the held copy and its message |
| `I7`–`I9` | the Install test's `install from Homebrew` job and the Upgrade test's `upgrade with brew upgrade` job | each step green; the `background-items` artifact holds what `sfltool dumpbtm` listed for Phylode |
| `I7`, `I8` as published | after a release, `published.yml` by hand on `main`, with `from` set to the release before it | each step green: the real tap installs, answers, and `brew upgrade` brings the latest after the helper fetched its checksum from GitHub |

## Gotchas

- Never run `install`, `on`, `off`, `uninstall` or `forget` on the owner's Mac to check a
  change; they change launchd, DNS and system folders.
- The Install test's Apple validator step warns "cannot tell" while Apple's validator on the
  runners fails against 1.1.1.1 too; the delv step before it is what proves the DNSSEC
  passthrough, and it must show the same result through Phylode as through 1.1.1.1.
- A step that never ran is not a pass: check the step list in the log, not only the run's
  colour.
- The Homebrew jobs use test builds that fetch checksums from a folder on the runner
  (`PHYLODE_TEST_CHECKSUM_URL_PREFIX`, `src/build.rs`), because GitHub has none for a version
  not yet released; `phylode --version` says `(test build)` there.
- `pgrep -x phylode` can find the list update or the upgrade helper, which run the same
  program; `.github/scripts/service-pid.sh` asks launchd for the service's own process.
