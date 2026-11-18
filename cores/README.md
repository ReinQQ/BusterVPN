# Core Engine Binaries and Routing Databases

BusterVPN acts as an orchestrator and process supervisor for **Xray** and **Hysteria 2** cores, utilizing **Wintun** on Windows and native kernel `/dev/net/tun` on Linux.

## Required Assets

Place pre-compiled binaries and geo-routing data files in this directory for local development or packaging:

### Windows (x86_64)
```text
cores/
  xray.exe          # Xray-core executable (v1.8.x+)
  hysteria.exe      # Hysteria 2 executable (v2.x+)
  wintun.dll        # Wintun driver library (architecture-matched x64)
  geoip.dat         # MaxMind / LoyalSoldier GeoIP database (includes geoip:ru, geoip:private)
  geosite.dat       # Domain routing list (includes geosite:category-ru, geosite:ru, etc.)
```

### Linux (x86_64 / aarch64)
```text
cores/
  xray              # Linux ELF binary (chmod 0755)
  hysteria          # Linux ELF binary (chmod 0755)
  geoip.dat         # GeoIP database
  geosite.dat       # GeoSite database
```

> **Note**: On Linux, TUN mode uses the kernel's native `/dev/net/tun` driver and does not require `wintun.dll`. Ensure binaries have executable permissions (`chmod +x`).

## Resolution and Runtime Synchronization

During application initialization, `SessionCoordinator::prepare_runtime_assets` locates cores from candidate paths:
- Bundled application resources (`cores/` directory beside the application binary)
- User data directory (`%LOCALAPPDATA%\BusterVPN\cores` or `~/.config/bustervpn/cores`)
- System installation paths (`C:\Program Files\BusterVPN\cores`, `/usr/lib/bustervpn/cores`, `/opt/bustervpn/cores`)

Verified assets are synchronized into the runtime directory:
- Windows: `%LOCALAPPDATA%\BusterVPN\runtime\`
- Linux: `~/.config/bustervpn/runtime/`

## Automated Provisioning & Verification

If any engine binary or database file is missing or outdated:
1. BusterVPN automatically queries the update manifest (`PRIMARY_MANIFEST_URL` / `FALLBACK_MANIFEST_URL`).
2. Assets are downloaded with SHA-256 hash validation.
3. Files are swapped atomically to prevent corruption or partial execution.

> **Security Notice**: Do not commit third-party binaries to the Git repository (`cores/*.exe`, `cores/xray`, `cores/hysteria`, `cores/*.dll`, and `cores/*.dat` are gitignored). Always verify cryptographic signatures and hashes before distributing.
