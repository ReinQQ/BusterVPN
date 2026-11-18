# Core binaries

Place the downloaded Windows x64 (or ARM64, matching the app process) binaries here:

```
cores/
  xray.exe
  hysteria.exe
  wintun.dll
```

The build copies this directory into the `cores` subfolder beside `BusterVPN.exe`;
`AppPaths.CoreDirectory` resolves that folder and downloaded updates are installed
there. `wintun.dll` is required for TUN mode and must match the architecture of
the running app (x64 or ARM64).

Do not commit third-party binaries. Verify their signatures/hashes according to
your supply-chain policy before shipping. See the root README for versions and
runtime layout.
