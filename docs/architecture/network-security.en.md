# Network and Security Architecture

[Türkçe](network-security.md) · [English](network-security.en.md)

MakerOS is designed for secure local and remote administration over Wi-Fi, Ethernet, and USB-C ([`makerd/src/network.rs`](../../makerd/src/network.rs) and [`makerd/src/auth.rs`](../../makerd/src/auth.rs)).

## Connectivity

- **Wi-Fi, Ethernet & Hotspot:** Managed via NetworkManager with support for WPA2/WPA3, WPA-Enterprise, and Wi-Fi hotspot mode (default gateway `10.42.0.1` / `10.42.0.0`).
- **First-Boot Fallback Hotspot:** Automatically starts an open `MakerOS` setup network if no known network is reachable.
- **USB-C Gadget Networking:** Direct connection over USB-C at `10.42.0.1` (`maker-os.local`).

## Security Controls

- **Session & Server-Enforced Lock:** Authenticated via `HttpOnly; SameSite=Strict` cookies; locking the screen invalidates the server session so reloading the page cannot bypass the lock screen.
- **CSRF & Origin Checks:** State-changing API calls require the `X-Maker: 1` header, and WebSocket connections enforce `Origin == Host`.
- **Browser Proxy Isolation (`browse.rs`):** Blocks private/loopback IP ranges (SSRF protection) and serves proxied pages inside an isolated `CSP: sandbox`.
