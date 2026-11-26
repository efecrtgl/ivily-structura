# Installation

## Windows (recommended)

1. Download `Ivily Structura_<version>_x64-setup.exe` from
   [GitHub Releases](https://github.com/efecrtgl/ivily-structura/releases).
2. Optional: verify the download against `SHA256SUMS.txt`:
   ```powershell
   (Get-FileHash ".\Ivily Structura_0.1.0_x64-setup.exe" -Algorithm SHA256).Hash.ToLower()
   ```
3. Run the installer. It installs per user (no administrator rights) and installs the WebView2 runtime
   if it is missing.

**Portable:** `Ivily.Structura_<version>_x64_portable.zip` — extract and run. Portable builds do not
update themselves.

### Updates

*Settings → Updates → Check now*. Updates are signed; Structura refuses packages whose signature does
not match. Your data is backed up before an update installs.

### Uninstall

Use *Settings → Apps* in Windows. Your data in `%LOCALAPPDATA%\IvilyStructura` is kept; delete that
folder to remove it as well.

## Building from source

Requirements: Windows 10/11, [Rust](https://rustup.rs) (stable, MSVC toolchain), Node.js 22+,
Visual Studio Build Tools (C++), WebView2 runtime.

```powershell
git clone https://github.com/efecrtgl/ivily-structura
cd ivily-structura
npm ci
npm run tauri dev                       # development
npx tauri build                         # release installer (unsigned updater artifacts need a key, see RELEASING.md)
```

The dev server uses port 1450.
