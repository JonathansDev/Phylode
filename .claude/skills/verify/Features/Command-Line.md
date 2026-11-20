# Command line

The commands a person types on the Mac running Phylode, as listed by `phylode --help`:
changing what is blocked, reading the state, and getting a report safe to post publicly.
Installation commands are in [Installation](Installation.md); `report` is in
[Weekly summary](Weekly-Summary.md).

## Sub-features

- `C1` `phylode allow <domain>` allows a name within seconds and takes it off the user's
  block list; `phylode block <domain>` does the reverse. Each undoes the other.
- `C2` `phylode status` shows the configuration, the served networks, the lists and the
  journal level.
- `C3` `phylode diagnostics` prints versions, states, counts and recent events with no
  domains, addresses or home folder names.
- `C4` `phylode --version` (`-V`, `version`) prints the version in `Cargo.toml`.
- `C5` A wrong command (`phylode frobnicate`, `phylode allow` with no domain, `phylode
  report 0`) is refused with exit code 2 and the usage.

## How to get to it

- Terminal on the Mac: `phylode <command>`; on a real install it reads
  `/usr/local/etc/phylode.conf`.
- Over SSH, the same commands.
- Scratch: `./target/release/phylode --config "$W/c.conf" <command>`.

## Driving it

Preconditions: the baseline in [Features.md](../Features.md); for `C1`, the instance is
running so the change can be seen in answers.

| Action | Command | Observe |
|---|---|---|
| `C1` block, then allow | `phylode --config "$W/c.conf" block example.org`, wait 3 s, `dig @::1 -p 15353 example.org +short`; then `allow example.org`, wait 3 s, `dig` again | a one-line confirmation each; `0.0.0.0`, then a real address; `example.org` moves from `$W/block.txt` to `$W/allow.txt`; running `block` twice says nothing changed |
| `C2` status | `phylode --config "$W/c.conf" status` | `Configuration: $W/c.conf`, the served networks, one list with 1 entry, the journal level |
| `C3` diagnostics | `phylode --config "$W/c.conf" diagnostics` | versions and counts; `grep` the output for `example`, `::1` and your home folder name finds nothing |
| `C4` version | `phylode --version` | the `Cargo.toml` version |
| `C5` refusals | `phylode frobnicate; echo $?` and `phylode report 0; echo $?` | the usage or the reason, and `2` |

## Gotchas

- Without `--config`, every command reads the installed configuration and the real journal
  of the Mac's owner; never drive a scratch check without it.
- `allow` and `block` need `allowlist` and `user_blocklist` in the configuration; a scratch
  configuration without them makes both stop with an error naming the key.
- On a real install the list files are writable by administrators only, so `allow` from a
  standard account fails; that is intended, not a bug.
- `status` output holds the configuration and must not be pasted into a public issue; only
  `diagnostics` is written to be safe there.
