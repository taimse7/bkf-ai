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

## Stage 4 — Conversion UI Integration

Status: implemented; frontend verified locally, native macOS build pending GitHub Actions.

- The verified Rust BKC engine is connected to a Hebrew RTL conversion interface.
- A writable destination directory is selected through the native directory dialog.
- A single BKC row or all supported BKC books can be enqueued.
- The Rust worker processes jobs sequentially and emits per-file byte progress and aggregate progress.
- Queue state is persisted in Application Support; a job interrupted by app closure returns to
  `queued` and resumes when the app is reopened.
- Cancellation is cooperative between streaming chunks. Temporary output is removed and no final
  PDF is renamed into place after cancellation.
- Existing PDFs can be skipped or assigned an automatic numbered filename.
- Completed PDFs and the destination directory can be opened through macOS.
- Failed/cancelled/disconnected jobs can be retried and retain a technical report.
- BKF rows display: “הקובץ זוהה כ־BKF, אך טרם קיים מפענח מלא.” They never enter the converter.
- Paths, sizes and progress use `u64`, and the engine remains streaming for files above 4 GB.

### Stage 4 verification recorded

- `pnpm test`: passed, 2 tests.
- `pnpm build`: passed, TypeScript compiled and Vite transformed 21 modules.
- Rust tests could not be executed on this restored Linux workspace because no Rust toolchain is
  installed. New Rust tests cover existing-file skip/rename, Hebrew filenames, BKF rejection, and
  queue recovery after reopening; they must run in the native GitHub Actions build.
- Physical drive removal, real disk-full behavior, macOS privacy-denied destinations, cancellation
  during a real Golden conversion, and native open actions require the macOS build and real media.
# BKC repair path (2026-07-21)

- Received and hash-verified the matching pair `688840.book` / `688840.pdf`.
- Verified `688840.book` is identical to the earlier sample (SHA-256 `8ff5bda4...805ba`).
- Verified the supplied repaired PDF has 973 pages and the expected SHA-256
  `a6488e53...924747`.
- Added a real fallback path for unknown BKC prefixes: create a 200-byte-aligned
  PDF probe, run Ghostscript `pdfwrite`, validate the rewritten PDF, and only then
  atomically publish the output.
- Kept the exact verified profile for `674817.book`; its recovered PDF hash and
  506-page count were independently confirmed from the supplied source/output.
- Frontend tests pass (2/2), production frontend build passes, and
  `git diff --check` passes.
- Rust compilation still requires GitHub Actions/macOS because this workspace has
  no Rust toolchain. Do not mark the repair path released until that build runs.
