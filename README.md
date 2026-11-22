# BusterVPN

[![Version](https://img.shields.io/badge/version-1.5.0-blue.svg?style=flat-square)](Cargo.toml)
[![Rust](https://img.shields.io/badge/Rust-2021_Edition-orange.svg?style=flat-square&logo=rust)](https://www.rust-lang.org/)
[![Tauri](https://img.shields.io/badge/Tauri-v2-24C8DB.svg?style=flat-square&logo=tauri)](https://tauri.app/)
[![Svelte](https://img.shields.io/badge/Svelte-5-FF3E00.svg?style=flat-square&logo=svelte)](https://svelte.dev/)
[![Tailwind CSS](https://img.shields.io/badge/Tailwind_CSS-3.4-38B2AC.svg?style=flat-square&logo=tailwind-css)](https://tailwindcss.com/)
[![License](https://img.shields.io/badge/License-MIT-green.svg?style=flat-square)](LICENSE)

Desktop VPN client and process supervisor for Windows and Linux. Built with Rust, Tauri v2, Svelte 5, and Tailwind CSS. Supports Sing-box, Xray-core, and Hysteria 2 engines with kernel-level process containment and routing table management.

---

## Architecture Overview (v1.5.0)

BusterVPN decouples privileged network configuration from the unprivileged user interface. The system comprises five modular components:

```
+------------------------------------------------------------------------+
| User Session (Unprivileged)                                            |
|                                                                        |
|  +---------------------------+       +------------------------------+  |
|  |       buster-app          |       |          busterctl           |  |
|  |  Tauri v2 + Svelte 5 GUI  |       |        CLI Utility           |  |
|  | (asInvoker, no UAC popup) |       |  (status, connect, service)  |  |
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
                                     | Supervises
+------------------------------------v-----------------------------------+
| External Network Cores                                                 |
|                                                                        |
|  +--------------------+  +-------------------+  +-------------------+  |
|  |    sing-box.exe    |  |     xray.exe      |  |   hysteria.exe    |  |
|  | (.srs rule-sets,   |  | (VLESS Reality,   |  | (Port hopping,    |  |
|  |  TUIC, WireGuard,  |  |  Trojan, VMess,   |  |  Salamander obfs, |  |
|  |  ShadowTLS, AnyTLS)|  |  Shadowsocks)     |  |  Brutal CC)       |  |
|  +--------------------+  +-------------------+  +-------------------+  |
+------------------------------------------------------------------------+
```

1. **`buster-service`**: Windows Service (`SERVICE_WIN32_OWN_PROCESS`) managing Wintun TUN interfaces, physical gateway discovery, static `/32` bypass routing, Windows Firewall rules, and core child processes inside Win32 Job Objects. Exposes a local Named Pipe IPC server (`\\.\pipe\BusterVPNService`) with permissions granted to Interactive and Authenticated Users. Supports auto-connecting the tunnel at system boot prior to user authentication.
2. **`buster-app`**: Tauri v2 desktop application running with standard user privileges (`asInvoker` in manifest). Delegates privileged routing, adapter, and firewall actions to `buster-service` via Named Pipe IPC when available; retains direct elevated fallback if the service is absent. Manages user-session WinINET proxy settings (`HKCU`).
3. **`busterctl`**: Lightweight command-line utility communicating directly with `buster-service` over Named Pipe IPC for headless automation, scripting, and service lifecycle management (`status`, `connect`, `disconnect`, `logs`, `service install|uninstall|start|stop`).
4. **`buster-core`**: Platform-agnostic configuration builder and link parser library supporting Sing-box JSON, Xray JSON, and Hysteria YAML formats.
5. **`buster-windows`**: Native Windows API bindings for DPAPI encryption, Job Objects, routing tables, COM Windows Firewall, Named Pipe IPC, and Service Control Manager interactions.

---

## Protocol and Core Support

BusterVPN supports two operational engine configurations:

### 1. Unified Sing-box Core (`sing-box.exe`)
Handles all modern proxy protocols in a single executable using memory-efficient compiled `.srs` (`rule_set`) binary databases (`geosite-ru.srs`, `geoip-ru.srs`) instead of legacy `.dat` files:
- **TUIC v5**: QUIC-based transport with congestion control selection (`bbr`, `cubic`), UUID/token authentication, and ALPN negotiation.
- **WireGuard**: Native kernel-mode TUN or userspace driver configuration with private keys, peer endpoints, pre-shared keys, allowed IPs, and MTU tuning.
- **ShadowTLS & AnyTLS**: Multi-stage TLS handshakes camouflaged behind legitimate third-party TLS servers.
- **VLESS Reality / XTLS-Vision**: uTLS client fingerprinting (`chrome`, `safari`, `edge`), fallback SNI validation, and multiplexing.
- **Hysteria 2**: UDP port-hopping ranges, Salamander obfuscation passwords, and upload/download bandwidth hints.
- **Trojan, Shadowsocks (AEAD 2022 & legacy), VMess**.

### 2. Classic Dual-Engine Core (`xray.exe` + `hysteria.exe`)
Traditional setup pairing Xray-core for TCP/TLS transports with Hysteria 2 for high-loss UDP links, using `geosite.dat` and `geoip.dat`.

---

## Operating Modes

- **TUN Mode (Virtual Network Adapter)**: Routes system traffic through a `wintun.dll` TUN interface. Pins the server IP via a `/32` physical interface route, sets adapter metric priority, and configures leak-free DNS resolvers.
- **System Proxy Mode**: Configures WinINET per-user HTTP/HTTPS/SOCKS5 proxy settings in `HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings` with live notify via `InternetSetOptionW`.
- **Dual Mode (TUN + Proxy)**: Simultaneously establishes both Wintun routing and system proxy for environments where specific software ignores non-proxy network routes.
- **Process Split Tunneling**: Per-application routing rules allowing selected executables (`.exe`) to bypass the VPN or route strictly through the tunnel interface.

---

## Security and Containment

- **Fail-Closed Windows Firewall Kill Switch**: Modifies Windows Filtering Platform rules via COM `INetFwPolicy2` to block outbound traffic by default, whitelisting only loopback, local subnet DHCP/DNS, core binaries, and pinned `/32` server IP destinations.
- **Win32 Job Object Containment**: Subordinate core processes run inside a Job Object with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`. Process termination of the parent or service guarantees immediate kernel-level termination of child core processes.
- **DPAPI Credential Storage**: Profile configurations, private keys, and subscription links are encrypted at rest using Windows DPAPI (`CryptProtectData`) with purpose-specific entropy salts.
- **Streamer IP Masking**: Interface toggle persists masked IP display state in `localStorage` to prevent accidental exposure during screenshots or live streaming.
- **WebRTC & DNS Leak Verification**: Diagnostic routines evaluate active DNS resolvers and execute STUN ICE gathering (`stun:stun.l.google.com:19302`) to verify that no non-tunnel public IP addresses leak.

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
   cargo tauri build
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
│       ├── app.manifest             # asInvoker execution level manifest
│       ├── installer-hooks.nsi      # NSIS service installation hooks
│       ├── src/                     # Tauri command handlers, AppState, SessionCoordinator
│       └── frontend/                # Svelte 5 + Tailwind CSS web interface
└── docs/
    └── ARCHITECTURE.md              # Architectural specification and subsystem boundaries
```

---

## License

MIT License. See [LICENSE](LICENSE) for details.
