# DNS answers

What every device on the network meets without knowing Phylode is there: names resolve as
usual, a blocked name resolves to `0.0.0.0` (`::` for AAAA) for two seconds, Apple's own
services always resolve, and a source outside the served networks gets no reply at all.

## Sub-features

- `A1` A served device gets the upstream's answer, from cache on the second ask.
- `A2` A listed name gets `0.0.0.0` / `::` with TTL `block_ttl_secs`; other types get NOERROR
  with SOA; never REFUSED.
- `A3` Apple's hard-coded hosts (`captive.apple.com`, `*.push.apple.com`, the Private Relay
  and iCloud probe hosts in `policy/allowlist.rs`) resolve even when a list names them.
- `A4` An answer pointing into the network (rebinding) is dropped and answered NOERROR with
  SOA; `internal_domain` exempts a name.
- `A5` A source outside the served networks, or over the rate limit, gets no reply.
- `A6` A device that asks for DNSSEC (`+dnssec`) gets the signatures and proofs unchanged;
  an upstream signature failure is answered SERVFAIL and logged `dnssec-bogus`.
- `A7` With no list loadable, everything is allowed and nothing of it is cached.

## How to get to it

- Any DNS query to the listening address: UDP, and TCP (`dig +tcp`).
- On a real install, port 53 on loopback, this Mac's private addresses and any
  `serve_network`; in a scratch instance, the `listen` address only.

## Driving it

Preconditions: the baseline in [Features.md](../Features.md); the instance answers.

| Action | Command | Observe |
|---|---|---|
| `A1` ask twice | `dig @::1 -p 15353 example.com` twice | an address both times, the second TTL never higher than the first; two `allowed` lines in `$W/journal/<day>.tsv` |
| `A2` ask a listed name | `dig @::1 -p 15353 ads.example.com` and `... AAAA` and `... TXT` | `0.0.0.0`, `::`, NOERROR with SOA; TTL 2; `blocked` lines |
| `A3` list an Apple host | add `captive.apple.com` to `$W/list.txt` by `mv` of a new file, wait for `blocklist-loaded`, then `dig @::1 -p 15353 captive.apple.com` | a real address, not `0.0.0.0` |
| `A4` rebinding | query a public name known to resolve to a private address | NOERROR, no answer records, SOA TTL 2; a `rebind` line |
| `A5` outside source | `dig -b <public IPv6> @<public IPv6> -p 15353 example.com` | no reply; an `outside-client` event (run the control: SKILL.md) |
| `A6` signatures | `dig @::1 -p 15353 isc.org DNSKEY +dnssec` and `dig @::1 -p 15353 dnssec-failed.org`; for the whole chain, `delv @::1 -p 15353 github.com` (BIND's validator, `brew install bind`) | RRSIG records, without the `ad` flag, since Phylode passes signatures on and does not validate them itself; SERVFAIL and a `dnssec-bogus` line; `phylode --config "$W/c.conf" allow dnssec-failed.org` then a reply; `delv` prints `; unsigned answer`, as it does against 1.1.1.1 |
| `A7` no list | replace `$W/list.txt` with an unreadable file before start, then query `ads.example.com` | a real address; `fail-open` lines; a `blocklist-failed` event |

Query lines are tab-separated: time, device, domain, type, outcome, reason. The outcome is
one of `blocked`, `allowed`, `fail-open`, `failed`, `overloaded`, `servfail`,
`dnssec-bogus`, `rebind`, `local`.

## Gotchas

- Apple's own validator (`requiresDNSSECValidation`) cannot prove `A6` on the CI runners:
  their mDNSResponder builds of 2026-10-12 never finish proving a name unsigned, whatever the
  resolver. Use `delv`, and compare with 1.1.1.1.

- 5353 belongs to mDNSResponder; the scratch port is 15353.
- Loopback is never rate-limited, so `A5`'s rate limit needs a served private address
  (`dig -b <addr> @<addr>`), or CI's `live.yml`, which drives 5003 source addresses.
- A blocked answer lives two seconds in the device's cache; wait longer than that before
  checking that an `allow` took effect.
- The firewall drops off-loopback packets to a binary it has not allowed; that looks like
  `A5` passing. Never change firewall settings to get a test through.
