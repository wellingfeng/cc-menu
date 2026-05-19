# CC Menu

CC Menu is a local control plane for AI coding context menus. It turns a small
Explorer/Finder entry into tested Rust logic for:

- default native launches for Claude Code, Codex, Gemini, and CC-Menu;
- agent registry and menu-cache generation;
- Start with Options launches through a local OpenAI-compatible gateway;
- fixed, fallback, race, and broadcast gateway routing;
- cc-switch dry-run imports;
- session indexing and cross-agent resume decisions;
- Desktop route switching diagnostics; and
- Windows/macOS adapter output without coupling platform APIs to core logic.

The implementation is intentionally conservative around system integration:
the core produces auditable menu and registry artifacts, and the installer can
place them in a per-user install directory. Tests exercise the whole flow using
temporary directories instead of mutating Explorer/Finder state.

## Quick Start

```powershell
cargo run -p cc-menu -- init
cargo run -p cc-menu -- menu sync
cargo run -p cc-menu -- menu print --format json
cargo run -p cc-menu -- launch --agent codex --cwd . --mode native --dry-run
cargo run -p cc-menu -- gateway chat --strategy fixed --prompt "hello"
```

## Build And Test

```powershell
cargo fmt --all
cargo test --workspace
powershell -ExecutionPolicy Bypass -File scripts\self-test.ps1
```

Build release artifacts and the Windows installer:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\build-release.ps1
```

The installer is a standalone executable named `cc-menu-setup-win-x64.exe`.
Run `cc-menu-setup-win-x64.exe --self-test` to verify it can install into a
temporary directory, execute the bundled CLI, and uninstall cleanly.
When double-clicked with no arguments, the installer leaves its console window
open after installation so the result and next command are visible.
On Windows it also writes current-user Explorer context menu entries under
`HKCU\Software\Classes\Directory\Background\shell` and
`HKCU\Software\Classes\Directory\shell`.

## GitHub Release

After `scripts\build-release.ps1` succeeds:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\publish-release.ps1 -Repo wellingfeng/cc-menu
```

The script creates a timestamped tag, uploads the installer, portable zip,
manifest, and release notes, then verifies the GitHub Release assets.
