# 🚀 rusty

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Language: Rust](https://img.shields.io/badge/Rust-1.75%2B-orange.svg)](https://www.rust-lang.org)
[![Platform](https://img.shields.io/badge/Platform-Linux%20%7C%20SteamOS-lightgrey.svg)]()
[![Rootless](https://img.shields.io/badge/Security-100%25%20Rootless-success.svg)]()

> **A blazing-fast, independent, rootless package manager for any Linux distribution.**

`rusty` was created to free developers and Linux users from distro-specific packaging bureaucracy (AUR registration freezes, PPA deadlocks, Flatpak bloat, or manual `git clone` clutter). 

It installs packages directly into user space (`~/.local/bin`), configures desktop entries (`.desktop`, icons, MIME types) automatically, and provides clean, atomic lifecycle management without requiring `sudo`.

---

## ✨ Why rusty?

| Feature | `rusty` | `pacman` / `apt` | `git clone` |
| :--- | :---: | :---: | :---: |
| **No `sudo` required** | ✅ Yes | ❌ Requires root | ✅ Yes |
| **Cross-distro portable** | ✅ Arch, Ubuntu, Fedora, SteamOS | ❌ Single distro | ⚠️ Manual compile |
| **Desktop / App Launcher sync** | ✅ Automatic (`.desktop`, MIME) | ⚠️ Distro-level | ❌ No |
| **One-command clean uninstall** | ✅ `rusty remove <pkg>` | ✅ Yes | ❌ Manual file hunting |
| **Single-second binary install** | ✅ Yes (zstd / prebuilt) | ⚠️ Depends | ❌ Slow source compile |
| **No gatekeeping / registration locks** | ✅ Open decentralized manifests | ❌ Centralized (AUR 503s) | ✅ Yes |

---

## 📥 Installation

### Option 1: Quick Install via Cargo
```bash
cargo install --git https://github.com/Kirieshka14/rusty
```

### Option 2: Build from Source
```bash
git clone https://github.com/Kirieshka14/rusty.git
cd rusty
cargo build --release
install -Dm755 target/release/rusty ~/.local/bin/rusty
```

Make sure `~/.local/bin` is in your `$PATH`:
```bash
export PATH="$HOME/.local/bin:$PATH"
```

---

## 🛠️ Usage

### Install a package
```bash
# Install from registry
rusty install rustydroid

# Or using shorthand alias
rusty i rustydroid

# Force reinstall
rusty install rustydroid --force

# Install from a local manifest
rusty install ./rusty.toml
```

### Search and inspect
```bash
# Search registry
rusty search android

# Inspect package details
rusty info rustydroid
```

### List installed packages
```bash
rusty list
# or
rusty ls
```

### Update packages
```bash
# Update all installed packages
rusty update

# Update a specific package
rusty update rustydroid
```

### Clean uninstall
```bash
rusty remove rustydroid
# or
rusty rm rustydroid
```
*Removes all binaries from `~/.local/bin`, unlinks `.desktop` files, cleans MIME associations, and updates application launchers.*

---

## 📦 For Package Maintainers: Creating a Package

A package in `rusty` is defined by a simple, human-readable `rusty.toml` file:

```toml
[package]
name = "rustydroid"
version = "0.1.0"
description = "Android APK/XAPK runner with Hyprland window sync"
author = "xkiri <riz.riz1941@gmail.com>"
license = "MIT"
homepage = "https://github.com/Kirieshka14/rustydroid"
repository = "https://github.com/Kirieshka14/rustydroid"

[sources.x86_64-linux]
url = "https://github.com/Kirieshka14/rustydroid/releases/download/v0.1.0/rustydroid-0.1.0-1-x86_64.pkg.tar.zst"
sha256 = "b1aad0141a12c1aaff94c84ab3113aa834f07aa481c7a17a66a5380b53ba0c91"
format = "tar.zst"

[install]
binaries = ["usr/bin/rustydroid"]
desktop = ["usr/share/applications/rustydroid.desktop"]
mime = ["usr/share/mime/packages/rustydroid-android-bundles.xml"]

[dependencies]
bins = ["waydroid", "adb"]
```

### Package your project using `rusty pack`
```bash
rusty pack --out dist/
```
This generates a production-ready `.tar.zst` payload with checksum calculated, ready to be attached to your GitHub / GitLab Releases!

---

## 🏗️ Architecture

```
~/.local/
├── bin/
│   └── rustydroid ────► (Symlink to store)
├── share/
│   ├── applications/
│   │   └── rustydroid.desktop
│   └── rusty/
│       ├── installed/
│       │   └── rustydroid-0.1.0/
│       │       └── (Package store files)
│       ├── cache/
│       │   └── (Downloaded archives)
│       └── state.json
```

---

## 📄 License

Distributed under the **MIT License**. See [LICENSE](LICENSE) for details.
