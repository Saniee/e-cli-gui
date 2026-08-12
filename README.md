# E-Cli-GUI

[![Build binaries](https://github.com/Saniee/e-cli-gui/actions/workflows/build.yml/badge.svg)](https://github.com/Saniee/e-cli-gui/actions/workflows/build.yml) <br>

Built directly on top of [e-cli](https://codeberg.org/Saniee/e-cli).

What it can do:

- [x] Downloading Favourites from a Username
- [x] Downloading Posts with specified Tags
- [x] Downloading Multiple Pages of Posts, either Favourites or with Tags.. or combined!
- [x] Downloading a Pool, with files numbered to preserve reading order
- [x] Packaging a downloaded Pool into a `.zip`/`.7z`/`.cbz` archive (requires `7z` on `PATH`)
- [x] Login with your API Key to download every post!
- [x] Resumable downloads with configurable retries and cooperative cancellation
- [x] Dry-run planning without writing files or local state
- [x] JSON metadata manifests and persistent failed-download manifests
- [x] Persistent MD5 duplicate detection
- [x] TOML tag preset loading and saving
- [x] Retry failed downloads from the GUI

# Usage
## Just run the .exe or the linux binary (without any extension).

# Downloads
Official builds are attached to [Releases](https://github.com/Saniee/e-cli-gui/releases) — grab `e-cli-gui-windows-x64` (the `.exe`) or `e-cli-gui-linux-x64` (the native binary).

Nightly builds are produced by [GitHub Actions](https://github.com/Saniee/e-cli-gui/actions/workflows/build.yml) for Linux and Windows. Open the newest successful run, scroll to the **Artifacts** section, and download `e-cli-gui-windows-x64` or `e-cli-gui-linux-x64`.
