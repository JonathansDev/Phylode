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
- `I6` The daily list update at 04:17 swaps in a complete new list and sets aside one with
  under half the entries.

## How to get to it

- Terminal on the Mac, with `sudo`, from the folder built per the README.
- The daily update runs from launchd (`deploy/`).

## Driving it

Preconditions: a branch pushed to GitHub. Every row runs in CI; read the run log.

| Action | Command | Observe |
|---|---|---|
| `I1`–`I4` | open a pull request, or run `install.yml` by its manual trigger on the branch | each step green in the Install test; the refusal step prints `Did not finish: ... Nothing was installed` |
| `I5` | the Upgrade test (`upgrade.yml`) on the pull request | it installs the previous release and updates by the README's steps |
| `I6` | `bash deploy/update-blocklists.sh "$W"` locally downloads the real lists into a scratch folder; edit a copy to test its limits | the new list in `$W`, or the held copy and its message |

## Gotchas

- Never run `install`, `on`, `off`, `uninstall` or `forget` on the owner's Mac to check a
  change; they change launchd, DNS and system folders.
- The Install test's DNSSEC step warns "cannot tell" when its 1.1.1.1 control fails on the
  runner; that step then proved nothing about Phylode.
- A step that never ran is not a pass: check the step list in the log, not only the run's
  colour.
