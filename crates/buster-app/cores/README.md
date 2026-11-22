# Core Engine Binaries and Routing Databases

BusterVPN v1.5.1 acts as an orchestrator and process supervisor for the **sing-box** core, utilizing **Wintun** on Windows and native kernel `/dev/net/tun` on Linux.

## Required Assets

Place pre-compiled binaries and geo-routing rule-set files in this directory for local development or packaging:

### Windows (x86_64)
```text
cores/
  sing-box.exe      # sing-box executable (v1.12.0+ / v1.14.0+)
  wintun.dll        # Wintun driver library (architecture-matched x64)
  geosite-ru.srs    # sing-box compiled binary rule-set for Russian domains
  geoip-ru.srs      # sing-box compiled binary rule-set for Russian IP ranges
```

### Linux (x86_64 / aarch64)
```text
cores/
  sing-box          # sing-box Linux ELF binary (chmod 0755)
  geosite-ru.srs    # sing-box compiled binary rule-set for Russian domains
  geoip-ru.srs      # sing-box compiled binary rule-set for Russian IP ranges
```

> **Note**: On Linux, TUN mode uses the kernel's native `/dev/net/tun` driver and does not require `wintun.dll`. Ensure binaries have executable permissions (`chmod +x`).

## Resolution and Runtime Synchronization

During application initialization, `SessionCoordinator::prepare_runtime_assets` locates cores from candidate paths:
- Bundled application resources (`cores/` directory beside the application binary)
- User data directory (`%LOCALAPPDATA%\BusterVPN\cores` or `~/.config/bustervpn/cores`)
- System installation paths (`C:\Program Files\BusterVPN\cores`, `/usr/lib/bustervpn/cores`, `/opt/bustervpn/cores`)
- System Service runtime directory (`C:\ProgramData\BusterVPN\runtime`)

Verified assets are synchronized into the runtime directory:
- Windows: `%LOCALAPPDATA%\BusterVPN\runtime\` and `C:\ProgramData\BusterVPN\runtime\`
- Linux: `~/.config/bustervpn/runtime/`

## Automated Provisioning & Verification

If any engine binary or database file is missing or outdated:
1. BusterVPN automatically queries the update manifest (`PRIMARY_MANIFEST_URL` / `FALLBACK_MANIFEST_URL`).
2. Assets are downloaded with SHA-256 hash validation.
3. Files are swapped atomically to prevent corruption or partial execution.

> **Security Notice**: Do not commit third-party binaries to the Git repository (`cores/*.exe`, `cores/sing-box`, `cores/*.dll`, and `cores/*.srs` are gitignored). Always verify cryptographic signatures and hashes before distributing.
