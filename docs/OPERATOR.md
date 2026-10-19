# dumbdns-rs Operator Security Guide

dumbdns-rs is a Rust DNS server that answers TXT queries via an LLM backend.
It listens on UDP/TCP for DNS and optionally serves DNS-over-TLS (DoT),
DNS-over-HTTPS (DoH), and DNS-over-QUIC (DoQ).

## 1. Firewalling and Bind Address

- Default `--dns-listening-address` binds to all interfaces. Prefer binding to
  `127.0.0.1` or a VPN/private address unless you intentionally run a public resolver.
- Why: an open UDP/TCP DNS port exposes the LLM backend to the internet. Each
  query can trigger LLM inference (cost/latency) and the allowlist/rate-limit
  (see Threat Model) are weak against spoofed sources.
- Restrict inbound UDP/TCP port 53 with host firewall rules (iptables/nftables,
  security groups) to known client ranges. Do the same for any
  `--dot/doh/doq-address` and `--metrics-address` ports.
- Run `--metrics-address` on localhost only; it is unauthenticated.

## 2. TLS Key and Certificate Setup (DoT / DoH / DoQ)

- Provide PEM files with `--tls-cert <cert.pem> --tls-key <key.pem>`.
  Both DoT (`--dot-address`), DoH (`--doh-address`), and DoQ (`--doq-address`)
  use the same pair.
- Use a certificate valid for `--dns-hostname` (e.g. `dns.example.com`) from a
  public CA or your internal CA when clients can validate it.
- If no files are given, the server generates an ephemeral self-signed
  certificate at startup. This encrypts traffic but provides no identity:
  clients must pin the fingerprint out-of-band (logged at startup, TOFU) or
  connections remain vulnerable to MITM.
- Fingerprint pinning: distribute the SHA-256 SPKI/cert fingerprint securely
  (config management, not DNS) and configure DoT/DoH/DoQ clients to check it.
  Re-pinning is required on every restart while using ephemeral keys; prefer
  stable PEM files in production.
- Protect the key file with filesystem permissions (`0600`, owned by the
  service user) and rotate on suspected compromise.

## 3. DNSSEC (ED25519)

- On-demand signing uses an ED25519 key loaded via `--dnssec-key <path>`.
- Generate with:

  ```sh
  openssl genpkey -algorithm ed25519 -out dnssec-ed25519.pem
  chmod 600 dnssec-ed25519.pem
  ```

- Publish the corresponding DS record at the parent zone for the chain of
  trust. Keep the PEM offline-backed-up and readable only by the service user.

## 4. Token-Leak Warning

- DNS QNAMEs on classic UDP/TCP transports are cleartext: any on-path observer,
  recursive resolver, or authoritative server on the delegation path can see the
  full query name.
- Never embed secrets (`--auth-token` values, API keys, `--doh-bearer` tokens)
  in query names sent over UDP/TCP.
- `--doh-bearer <token>` (DoH `Authorization: Bearer`) is only safe over
  encrypted transports (DoH itself, or via VPN/TLS-protected network). Sending
  the same token or name content over plain UDP/TCP negates its protection.

## 5. Threat Model

- Spoofable UDP source IPs: `--allowlist` (IP/CIDR matching) and `--rate-limit`
  operate on the observed source address. UDP sources are trivially spoofable,
  so these are rate-shaping/hygiene, not authentication. Use TSIG-equivalents,
  VPN identity, or encrypted transports with client auth where spoofing matters.
- LLM cost abuse per query: every cache miss for a TXT query calls
  `--llm-endpoint`. An attacker who can reach the DNS port can burn GPU/API
  budget and cause latency. Mitigate with firewalling (Section 1), tight
  `--allowlist`, low `--rate-limit` values, and LLM-side quotas/auth.
- Cache poisoning via high TTLs: signed/LLM answers are cached. A poisoned or
  attacker-influenced LLM answer persists for the TTL duration. Stateful
  (C2-duplex / session) answers therefore use a short 5s TTL
  (`--ttl-stateful` default 5) to bound stale/malicious reuse; keep
  `--ttl-stateless`/`--ttl-stateful` low unless you understand the tradeoff.
- Additional notes: `--config` files may contain tokens (lock down with `0600`);
  `--smuggle-a`/`--smuggle-aaaa` encode data in address records visible to any
  observer; treat smuggled content as public.

## 6. C2-Duplex Usage and Abuse Warning

- The stateful C2-duplex mode (short-TTL TXT answers plus `--smuggle-a` /
  `--smuggle-aaaa` exfiltration/infiltration channels) is dual-use: it can be
  used for legitimate testing, research, and data-transport experiments, and it
  can also be abused as command-and-control infrastructure.
- Only run C2-duplex on networks and systems you own or are explicitly
  authorized to test (written permission, defined scope and time window).
- Do not point it at third-party resolvers or domains without authorization,
  do not use it to evade monitoring or policy controls, and comply with all
  applicable laws and organizational policies.
- For authorized work: isolate the test network, bind to VPN/localhost
  addresses, keep TTLs short, monitor `--metrics-address` and LLM logs, and
  tear down keys, tokens, and DNS delegations when finished.
