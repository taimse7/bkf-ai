# Progress

## Stage 1 — Mac Build Proof

Status: source implementation complete; native macOS verification pending.

- Tauri 2 shell with a Rust backend created.
- React + TypeScript frontend created with pnpm.
- Hebrew RTL is set at the HTML and CSS roots.
- The frontend calls a real Rust Tauri command (`build_proof`) on startup.
- A macOS Apple Silicon build workflow is included for future use, but has not run.
- The React production build passes in the current Linux environment.
- No BKC or BKF reading/decoding functionality is implemented in this stage.

The stage is not yet considered complete. A real Mac must successfully run the commands in
`BUILD_ON_MAC.md` and produce both the `.app` and `.dmg` artifacts.

## Verification recorded

- `pnpm install`: passed.
- `pnpm build`: passed; Vite transformed 18 modules and produced `dist/`.
- `pnpm tauri info`: configuration detected, but native verification could not run because the
  current host is Linux without Rust or the Linux Tauri system libraries.
- `cargo test`: not run; Rust is unavailable on the current host.
- `pnpm tauri build --target aarch64-apple-darwin --bundles app,dmg`: not run; macOS is required.

## Stage 2 — Scanner and Library UI

Status: implemented and core-tested; native Tauri integration build on macOS is pending.

- Native macOS directory/volume selection is wired through the Tauri dialog plugin.
- Scanning runs on a Rust background thread and only traverses the selected source.
- Source files are opened with read-only options; only the first 16 bytes are read.
- Files are classified by their leading bytes as `BKC`, `BKF`, or `Unknown`; extensions are ignored.
- SQLite is created under the app data directory (`Application Support` on macOS).
- Interrupted/cancelled/disconnected scans retain their scan id and resume unchanged entries.
- Cancellation, drive disconnection, read errors, and permission-denied statuses are represented.
- The UI requests paged SQLite results and renders only a fixed-height visible window.
- Selection state is persisted in SQLite.
- No conversion code was added.

### Stage 2 verification recorded

- `pnpm test`: passed, 2 tests, including a virtual window over 10,000 rows.
- `pnpm build`: passed, 21 modules transformed.
- `cargo test --manifest-path src-tauri/scanner-core/Cargo.toml`: passed, 4 tests.
  The tests use three on-disk binary fixtures, enforce the 16-byte prefix limit, reject
  extension-only identification, and page through 10,000 rows in a real temporary SQLite file.
- `cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check`: passed after formatting.
- Full Linux Tauri test: blocked before application compilation because the host lacks
  `pkg-config`/GTK development libraries.
- Apple Silicon cross-check from Linux: blocked by the absence of an Apple Objective-C compiler/SDK.
- Native `.app`/`.dmg` build: not run; a macOS host is still required.

### First native macOS run

- GitHub Actions run `29739837810` used macOS 14.8.7 on an Apple Silicon runner.
- Checkout, pnpm, Node.js, Rust setup, dependency installation, and frontend build passed.
- Rust compilation reached the application crate and failed because the required
  `src-tauri/icons/icon.png` asset was missing.
- A complete Tauri icon set was then generated from `app-icon.svg`; the native run must be
  repeated before stage 2 can be marked complete.

## Stage 3 — Verified BKC Conversion Engine

Status: implemented and verified against the supplied golden files.

- Filename search filters the virtual library through paged SQLite queries.
- The conversion engine is a standalone Rust core; no JavaScript decoding is used.
- BKC is required by magic bytes. BKF and unknown variants are rejected.
- `startxref`, the physical XRef object, and `baseOffset` are discovered from file structure.
- Decoder profile `bkc-golden-674817-v1` is selected only for the verified prefix fingerprint.
- Output is streamed to a temporary file, validated, synced, and atomically renamed after success.
- Golden result: `baseOffset=7105`, output size `115172663`, page count `506`, SHA-256
  `030B0E2B93270B96EF24D63F1C5254D41BA2B54C9E0232C428F2D9E254E3B165`.
- Streaming binary comparison against `674817_recovered.pdf`: identical.
- No BKF conversion was added.
