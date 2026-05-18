# Repository Instructions

The upstream request referenced `@RTK.md`, but `RTK.md` was not present in the
workspace when this repository was initialized. Keep future local instructions
in this file or add `RTK.md` at the repository root.

## Development

- Use Rust 2024 and keep shared behavior in `crates/cc-menu-core`.
- Platform-specific behavior must stay behind adapter modules.
- Run `cargo fmt --all`, `cargo test --workspace`, and
  `powershell -ExecutionPolicy Bypass -File scripts/self-test.ps1` before a
  release.
- Do not commit generated `release/`, `dist/`, or `target/` artifacts.
