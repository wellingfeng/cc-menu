# Implementation Map

This repository was created from `tasks/ai-coding-context-menu-plan.html`.
The original workspace did not contain the referenced `RTK.md`, so the plan
HTML is the source requirement document.

## Requirement Coverage

| Requirement | Implementation |
| --- | --- |
| FR-1 default four menu entries | `cc_menu_core::menu::build_menu_cache` and unit tests assert Claude Code, Codex, Gemini, CC-Menu only. |
| FR-2 native launch boundary | `cc_menu_core::launch::plan_launch` avoids Gateway env for `LaunchMode::Native`. |
| FR-3 CC-Menu submenu, options, account switching, sync | `cc-menu menu`, `cc-menu launch --mode gateway`, `cc-menu sync`, and `cc-menu desktop switch`. |
| FR-4 gateway fixed/fallback/race/broadcast | `cc_menu_core::gateway` with tests for every strategy. |
| FR-5 cc-switch dry-run sync | `cc_menu_core::sync` and `cc-menu sync preview/apply`. |
| FR-6 unified settings data | `AppConfig` persists agents, gateway, sessions, desktop, terminal, and menu data. |
| FR-7 session indexing | `cc_menu_core::sessions` scans JSON manifests into SQLite and filters by project directory. |
| FR-8 automatic resume decision | `decide_resume_strategy` selects native resume, handoff, replay, or archive fallback. |
| FR-9 Desktop route switching | `cc_menu_core::desktop` updates active routes and reports `restart_required`. |
| FR-10 platform adapters | `cc_menu_core::platform` isolates Windows/macOS/Linux artifact generation. |
| FR-11 agent changes sync menu cache | `cc-menu sync apply` validates config and rebuilds `menu-cache.json`. |
| FR-12 agent card data model | Agent config includes adapter, provider, defaults, capabilities, and menu placement; the current implementation exposes this through JSON/CLI rather than a graphical settings shell. |

## Work Package Mapping

| Work Package | Implementation |
| --- | --- |
| W1 menu and launch | `menu`, `launch`, `platform` modules; `cc-menu platform generate`. |
| W2 agent cards and menu sync | `config`, `menu`, `sync`; data model is ready for a UI shell. |
| W3 agent sync and route switching | `sync`, `desktop`, `launch`. |
| W4 sessions and automatic resume | `sessions` module and SQLite index. |
| W5 gateway | `gateway` module plus `cc-menu gateway chat/serve`. |
| W6 context bridge | `export_standard_context` creates manifest, transcript, JSONL, files-touched, and summary artifacts. |
| W7 desktop switching diagnostics | `desktop` module reports hot reload vs restart-needed outcomes. |
| W8 macOS adapter | `MacOsAdapter` emits launchd plist and menu cache artifacts. |

## Validation

The release build script runs all required local validation:

```powershell
cargo fmt --all -- --check
cargo test --workspace
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\self-test.ps1
release\cc-menu-setup-win-x64.exe --self-test
```

The installer self-test installs the embedded CLI into a temporary directory,
runs `cc-menu self-test`, and removes the install directory.
