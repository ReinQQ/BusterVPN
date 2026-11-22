<div align="center">

# BusterVPN

![Version](https://img.shields.io/badge/version-1.5.1-blue)
![Rust](https://img.shields.io/badge/Rust-2021%20Edition-orange?logo=rust)
![Tauri](https://img.shields.io/badge/Tauri-v2-24C8DB?logo=tauri)
![Svelte](https://img.shields.io/badge/Svelte-5-FF3E00?logo=svelte)
![Tailwind](https://img.shields.io/badge/Tailwind%20CSS-3.4-38B2AC?logo=tailwind-css)
![Platform](https://img.shields.io/badge/Platform-Windows%2010%2F11%20(x64)-0078D6?logo=windows)
![License](https://img.shields.io/badge/License-MIT-green)

**Desktop VPN client and background process supervisor for Windows.**  
Built with Rust, Tauri v2, Svelte 5, and Tailwind CSS. Powered by a high-performance hybrid architecture: **Sing-box v1.14+** as the primary TUN, `.srs` routing, and multi-protocol engine, combined with an on-demand **Xray-core v26.11.9** local SOCKS5 sidecar for VLESS Reality post-quantum (`X25519MLKEM768`) handshakes.

</div>

## Architecture Overview (v1.5.1)

BusterVPN decouples privileged network configuration from the unprivileged user interface. The system comprises five modular components:

```
+------------------------------------------------------------------------+
| User Session (Elevated / requireAdministrator)                         |
|                                                                        |
|  +---------------------------+       +------------------------------+  |
|  |       buster-app          |       |          busterctl           |  |
|  |  Tauri v2 + Svelte 5 GUI  |       |        CLI Utility           |  |
|  |   (requireAdministrator)  |       |  (status, connect, service)  |  |
|  +-------------+-------------+       +--------------+---------------+  |
+----------------|------------------------------------|------------------+
                 | Named Pipe IPC                     |
                 | (\\.\pipe\BusterVPNService)         |
+----------------v------------------------------------v------------------+
| Windows Service (Privileged / SYSTEM)                                  |
|                                                                        |
|  +------------------------------------------------------------------+  |
|  |                        buster-service                            |  |
|  |  - Wintun virtual adapter lifecycle & /32 route pinning          |  |
|  |  - COM Windows Firewall Kill Switch (fail-closed policy)         |  |
|  |  - SafeJobObject supervision (KILL_ON_JOB_CLOSE)                 |  |
|  |  - Boot auto-connect tunnel (before user login)                  |  |
|  +---------------------------------+--------------------------------+  |
+------------------------------------|-----------------------------------+
                                     | Supervises (SafeJobObject)
+------------------------------------v-----------------------------------+
| Network Engine Layer                                                   |
|                                                                        |
|  +------------------------------------------------------------------+  |
|  |                    sing-box.exe (v1.14+)                         |  |
|  |  Primary TUN Engine, .srs Rule-Sets, DNS Hijacking,              |  |
|  |  Hysteria 2, TUIC v5, WireGuard, Trojan, SS, ShadowTLS, AnyTLS   |  |
|  +---------------------------------+--------------------------------+  |
|                                    | Detour on VLESS Reality (127.0.0.1:10810)
|                                    v                                   |
|  +------------------------------------------------------------------+  |
|  |                    xray.exe (v26.11.9)                           |  |
|  |  Local SOCKS5 Sidecar for Post-Quantum X25519MLKEM768 Reality   |  |
|  +------------------------------------------------------------------+  |
+------------------------------------------------------------------------+
```

1. **`buster-service`**: Privileged Windows Service (`SERVICE_WIN32_OWN_PROCESS`) managing Wintun TUN interfaces, physical gateway discovery, static `/32` bypass routing, Windows Firewall rules, and core child processes inside Win32 Job Objects. Exposes a local Named Pipe IPC server (`\\.\pipe\BusterVPNService`) accessible without UAC prompts. Supports automatic tunnel connection at system boot prior to user login.
2. **`buster-app`**: Tauri v2 desktop application configured with `requireAdministrator` manifest. Delegates privileged routing, adapter, and firewall actions to `buster-service` via Named Pipe IPC when available; seamlessly handles local Wintun TUN, routing, and Kill Switch directly out-of-the-box if the background service is absent. Manages user-session WinINET proxy settings (`HKCU`).
3. **`busterctl`**: Lightweight command-line utility communicating directly with `buster-service` over Named Pipe IPC for headless automation, scripting, and service lifecycle management (`status`, `connect`, `disconnect`, `logs`, `service install|uninstall|start|stop`).
4. **`buster-core`**: Platform-agnostic configuration builder and link parser library supporting Sing-box JSON, Xray JSON, and Hysteria YAML formats.
5. **`buster-windows`**: Native Windows API bindings for DPAPI encryption, Job Objects, routing tables, COM Windows Firewall, Named Pipe IPC, and Service Control Manager interactions.

---

## Hybrid Engine & Protocol Support

BusterVPN v1.5.1 features an optimized hybrid engine combining `sing-box.exe` and `xray.exe`:

### 1. Primary Engine: Sing-box Core (`sing-box.exe` v1.14+)
Provides system-wide TUN routing and protocol handling using compiled `.srs` (`rule_set`) binary databases (`geosite-ru.srs`, `geoip-ru.srs`):
- **Hysteria 2**: Native UDP port-hopping ranges, Salamander obfuscation, and bandwidth hints.
- **TUIC v5**: QUIC transport with BBR/Cubic congestion control and ALPN negotiation.
- **WireGuard**: Kernel-mode TUN endpoints with private keys, pre-shared keys, allowed IPs, and MTU tuning.
- **ShadowTLS & AnyTLS**: Multi-stage TLS handshakes camouflaged behind legitimate third-party servers.
- **Trojan, Shadowsocks (AEAD 2022 & legacy), VMess**.
- **Leak-Free DNS**: System DNS hijacking with remote DoH/UDP resolver routing through proxy and domestic `.ru`/`.su` queries through local DNS.

### 2. Post-Quantum Sidecar: Xray-core (`xray.exe` v26.11.9)
Modern Xray servers (v26.9.8+ / v26.11.9+) enforce hybrid post-quantum key exchange (`X25519MLKEM768`) in Reality ClientHello. Upstream `sing-box`'s uTLS fork strips ML-KEM extension groups, causing VLESS Reality handshakes to fail.
- BusterVPN solves this seamlessly: when connecting to a VLESS Reality profile, the coordinator automatically writes a minimal standalone Xray config (zero `.dat` files and zero TUN needed) and launches `xray.exe` as a local SOCKS5 sidecar (`127.0.0.1:10810`) inside the same Win32 `SafeJobObject`.
- `sing-box` routes the proxy traffic through this localhost detour while retaining exclusive ownership of the Wintun adapter, `.srs` routing rules, DNS hijacking, and Kill Switch protection.
- All non-VLESS protocols connect directly through `sing-box.exe` in single-process mode.

---

## Operating Modes

- **TUN Mode (Virtual Network Adapter)**: Routes system traffic through a `wintun.dll` TUN interface. Pins the server IP via a `/32` physical interface route, sets adapter metric priority, and configures leak-free DNS resolvers.
- **System Proxy Mode**: Configures WinINET per-user HTTP/HTTPS/SOCKS5 proxy settings in `HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings` with live notify via `InternetSetOptionW`.
- **Dual Mode (TUN + Proxy)**: Simultaneously establishes both Wintun routing and system proxy for environments where specific software ignores non-proxy network routes.
- **Process Split Tunneling**: Per-application routing rules allowing selected executables (`.exe`) to bypass the VPN or route strictly through the tunnel interface.

---

## Security & Diagnostics

- **Fail-Closed Windows Firewall Kill Switch**: Modifies Windows Filtering Platform rules via COM `INetFwPolicy2` to block outbound traffic by default, whitelisting only loopback (`127.0.0.1/8`), local subnet DHCP/DNS, core binaries, and pinned `/32` server IP destinations.
- **Win32 Job Object Containment**: Subordinate core processes run inside a Job Object with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`. Process termination of the parent or service guarantees immediate kernel-level termination of child core processes.
- **DPAPI Credential Storage**: Profile configurations, private keys, and subscription links are encrypted at rest using Windows DPAPI (`CryptProtectData`) with purpose-bound entropy salts.
- **Streamer IP Masking ("Eye" Toggle)**: Dashboard toggle persists masked IP display state in `localStorage` across restarts to prevent accidental exposure during screenshots or live streaming.
- **WebRTC & DNS Leak Verification**: Fast concurrent diagnostic routines execute asynchronous STUN UDP probes (`stun.l.google.com:19302` / `stun1.l.google.com:19302`) with strict 1.2s timeouts and evaluate DNS resolvers to verify that no non-tunnel public IP addresses leak.
- **Inline SVG Service Icons**: Crisp, lightweight vector icons for major services (YouTube, Telegram, Discord, Steam, OpenAI, Twitch, etc.) across routing rules and logs.

---

## CLI Reference (`busterctl`)

```bash
# Query connection and service status
busterctl status

# Connect to the default or active profile
busterctl connect

# Connect to a specific profile by ID or name
busterctl connect --profile "Frankfurt-Reality"

# Disconnect active tunnel
busterctl disconnect

# Tail recent service logs
busterctl logs --tail 50

# Windows Service management (requires Administrator prompt)
busterctl service status
busterctl service install
busterctl service start
busterctl service stop
busterctl service uninstall
```

---

## Building from Source

### Prerequisites
- Windows 10 / 11 (x64) or Linux (x86_64)
- Rust 1.85+ with MSVC toolchain (`x86_64-pc-windows-msvc`)
- Node.js 20+ and npm
- Visual Studio Build Tools (C++ workload)

### Build Steps

1. **Clone repository and build backend crates:**
   ```bash
   git clone https://github.com/ReinQQ/BusterVPN.git
   cd BusterVPN/antivpnv2
   cargo build --release --workspace
   ```

2. **Build frontend assets:**
   ```bash
   cd crates/buster-app/frontend
   npm install
   npm run build
   cd ../../..
   ```

3. **Run unit and integration test suite:**
   ```bash
   cargo test --workspace
   ```

4. **Package Tauri desktop application:**
   ```bash
   cd crates/buster-app
   cargo tauri build --no-bundle
   ```

Binary artifacts are output to `target/release/`:
- `buster-app.exe` — Desktop GUI client
- `buster-service.exe` — Privileged background Windows Service
- `busterctl.exe` — Command-line management tool

---

## Project Structure

```
antivpnv2/
├── Cargo.toml                       # Workspace manifest
├── crates/
│   ├── buster-core/                 # Protocol models, share link parsers, config builders
│   ├── buster-windows/              # Win32 APIs: Named Pipe IPC, SCM, DPAPI, JobObject, routes
│   ├── buster-service/              # Background Windows Service binary (buster-service.exe)
│   ├── buster-ctl/                  # CLI management tool (busterctl.exe)
│   └── buster-app/                  # Tauri v2 desktop application
│       ├── app.manifest             # requireAdministrator application manifest
│       ├── installer-hooks.nsi      # NSIS service installation hooks
│       ├── src/                     # Tauri command handlers, AppState, SessionCoordinator
│       └── frontend/                # Svelte 5 + Tailwind CSS web interface
└── docs/
    └── ARCHITECTURE.md              # Architectural specification and subsystem boundaries
```

---

## License

MIT License. See [LICENSE](LICENSE) for details.
