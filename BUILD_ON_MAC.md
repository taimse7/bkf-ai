# Build on Apple Silicon Mac

These commands are for development/build verification on a Mac. End users will not need these
tools after a verified `.app` and `.dmg` are produced.

## Prerequisites

- Xcode Command Line Tools
- Rust stable toolchain
- Node.js 24
- pnpm 11.7.0

## Verification

```bash
pnpm install --frozen-lockfile
pnpm test
pnpm build
cargo test --manifest-path src-tauri/scanner-core/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml
pnpm tauri build --target aarch64-apple-darwin --bundles app,dmg
```

Expected artifacts:

```text
src-tauri/target/aarch64-apple-darwin/release/bundle/macos/BKF AI.app
src-tauri/target/aarch64-apple-darwin/release/bundle/dmg/*.dmg
```

This stage is successful only when every command exits successfully and both artifacts exist.
