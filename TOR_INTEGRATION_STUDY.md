# Tor integration study

This note captures the implementation path for adding Tor-aware downloads to gDownloader.

## Goals

- Per-download Tor mode: route selected downloads through Tor while the rest of the app uses the normal network.
- Circuit rotation: retry with a fresh Tor circuit when a host rate-limits or blocks a request.
- System-wide Tor mode with kill switch: optional advanced mode where all app traffic is forced through Tor and direct egress is blocked if Tor is unavailable.

## Recommended architecture

1. Bundle or detect a local Tor daemon.
   - Default SOCKS endpoint: `127.0.0.1:9050`.
   - Optional control endpoint: `127.0.0.1:9051`.
   - Store Tor state under the app data directory, not inside the project tree.

2. Add a network profile to each download.
   - `direct`: current behavior.
   - `tor`: reqwest/electron helper uses SOCKS5 through Tor.
   - Later: `system_tor`: only available when the kill switch is active.

3. Use SOCKS isolation per download.
   - Tor supports stream isolation using SOCKS credentials/tokens.
   - Generate a unique SOCKS username per download or per retry window so parallel downloads do not necessarily share a circuit.
   - Keep one logical download on one circuit unless rate-limit handling asks for rotation.

4. Add control-port rotation.
   - Authenticate to the control port with cookie or password auth.
   - Send `SIGNAL NEWNYM` when a retry policy requests a new circuit.
   - Respect Tor cooldown. Do not spam NEWNYM per chunk or per small retry.

5. Implement kill switch outside the normal provider layer.
   - App-level kill switch: refuse any request if the active network profile is Tor but the Tor SOCKS probe fails.
   - OS-level kill switch: optional and platform-specific:
     - macOS: `pf` anchor rules.
     - Linux: nftables/iptables owner or cgroup rules.
     - Windows: Windows Filtering Platform rules or firewall rules scoped to the app binaries.
   - OS firewall changes need explicit user approval and a rollback path.

## Risks

- Many file hosters block Tor exit nodes. Circuit rotation can help only when the block is exit-specific, not when the host blocks most Tor exits.
- DNS leaks: all hostname resolution must happen through SOCKS5/Tor. Do not pre-resolve hostnames in the app.
- Captcha/browser-helper providers need separate handling, because Chromium/Electron proxy settings must match the selected network profile.
- A broken kill switch can leave the user offline. Always implement dry-run, status checks, and rollback.

## Minimum viable implementation

1. Add `networkProfile` to downloads and provider context.
2. Extend `ProviderDefaults::http_client_with_proxy` to support per-request Tor credentials for stream isolation.
3. Add a Tor health endpoint: probe SOCKS, probe control port, expose current external IP through a lightweight check.
4. Add retry policy integration: rate-limit -> optional NEWNYM -> retry after cooldown.
5. Add a settings panel later, after the backend API is stable.

## References

- Tor stream isolation spec: https://spec.torproject.org/path-spec/stream-isolation.html
- Tor SOCKS extensions: https://spec.torproject.org/socks-extensions.html
- Tor control NEWNYM behavior via Stem docs: https://stem.torproject.org/api/control.html
- Tor kill switch concept in official support docs: https://support.torproject.org/es/tr/tor-vpn/kill-switch/
