# Install Hayai

## Installer

Download `hayai-v<version>-windows-x86_64-setup.exe` from the latest
[GitHub release](https://github.com/Ariaryy/hayai/releases) and run it. The
installer adds Start Menu and desktop shortcuts, registers Hayai to launch for
the current user at login, and bundles the matching Scry Search utilities.

Hayai is not code-signed yet. Windows SmartScreen may warn about the download.
Confirm that it came from this repository's Releases page and compare its
SHA-256 hash against `SHA256SUMS.txt` before choosing **Run anyway**.

## Portable package

Download and extract `hayai-v<version>-windows-x86_64-portable.zip`, then run
`hayai.exe`. The portable package contains the same application and Scry
utilities, but does not install shortcuts or register Hayai to launch at login.

## File-search setup

App search and the calculator work immediately. File search requires Scry
Search's elevated, per-user daemon. Enter file mode by pressing `Space` and then
typing `f`; the initial space activates Hayai's command-mode router. If the
daemon is not running, Hayai offers a setup action. Windows displays one UAC
prompt while the scheduled task is installed and started.

See [File search](file-search.md) for details.

## Upgrade

Run the newer Hayai installer. Velopack replaces the installed application,
while Scry's per-user index data remains available. Release packages include
the tagged Scry release matched to Hayai's Rust client dependency.

## Remove

Uninstall Hayai from **Settings → Apps → Installed apps**. The installer removes
Hayai's launch-at-login entry.

Scry Search uses its own per-user scheduled task and preserves its index data by
default. To remove the daemon task, run the bundled `uninstall-daemon.ps1` from
an elevated PowerShell session. See the
[Scry daemon guide](https://github.com/Ariaryy/scry-search/blob/main/docs/users/daemon.md)
for its data locations and complete removal behavior.

## Build from source

Requirements:

- Windows 10 or 11
- Rust 1.89 or newer
- Git
- the .NET SDK and Velopack CLI for installer builds

```powershell
git clone https://github.com/Ariaryy/hayai.git
cd hayai
cargo run --release
```

To create release packages:

```powershell
dotnet tool install -g vpk --version 0.0.1298
./scripts/build-installer.ps1
```

The packaging script downloads the tagged Scry package recorded in `Cargo.lock`;
a sibling checkout or Scry source build is not required. Outputs are written
under `dist/velopack/`.
