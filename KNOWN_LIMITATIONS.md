# Known Limitations

- `688840.book` is now parsed correctly through its `startxref`/XRef structure, but its encoded 200-byte PDF header uses a decoder profile that has not yet been safely reconstructed. It is recognized as BKC and rejected without producing a partial PDF rather than being reported as missing `startxref`.
- The compact conversion summary intentionally does not show every historical job on the main
  screen. Technical details remain available through the downloadable diagnostics log.

- BKC conversion currently supports only the decoder profile verified by `674817.book`.
  Other BKC variants are rejected rather than guessed.
- Full BKF decoding is not implemented or claimed.
- The generated macOS app is unsigned and not notarized, so Gatekeeper may warn when it is downloaded to another Mac.
- The CI artifact is built only for Apple Silicon (`aarch64-apple-darwin`), not Intel Macs.
- Native Finder launch is verified structurally by producing an `.app`; automated CI cannot prove a human double-click interaction.
- The current UI contains only a real frontend-to-Rust connection check.
- The supplied ZIP contains source code, not a ready-to-run `.app` or `.dmg`.
- Native macOS build verification is still pending because the current build host is Linux.
- Stage 2 recognizes a container only when bytes 0–2 are exactly ASCII `BKC` or `BKF`.
  The BKC leading signature is supported by the verified sample history; a production BKF
  specimen was not available in this workspace to independently confirm that this rule covers
  every BKF variant.
- Identification reads exactly up to the first 16 bytes; it does not search for signatures later
  in the file during the library scan.
- The three committed fixture files are real on-disk binary-prefix fixtures, but they are small
  classifier fixtures rather than full copyrighted production book containers.
- Resume avoids rereading unchanged file prefixes but still walks the selected directory to
  reconcile additions and deletions.
- Selecting a source that contains the app's own Application Support directory is rejected, so
  the SQLite database can never be written inside the selected source tree.
- Permission errors and disconnection paths are implemented, but physical drive removal and
  macOS privacy-permission behavior require validation on an actual Mac.
- The virtual library uses fixed-height rows and paged queries. Filename search is implemented;
  sorting controls and additional filters are not.
- The golden fixtures are external test inputs and are not committed to Git because together they
  are about 230 MB. The ignored integration test must be invoked explicitly with `BKF_GOLDEN_DIR`.
- The first native macOS workflow exposed a missing Tauri icon and therefore did not reach app/DMG
  bundling. Icons are now included, but the corrected native workflow has not yet run.
- Stage 4 persists and resumes the conversion queue, but recovery restarts the interrupted file from
  byte zero; it does not append to a partial temporary PDF.
- Free-space failure is reported from the operating-system write error. The app does not reserve the
  full output size in advance because another process can consume disk space during conversion.
- Progress covers streaming output bytes. Final PDF validation and SHA-256 verification are
  indeterminate rather than byte-progress phases.
- Native tests involving physical drive disconnection, a genuinely full disk, macOS folder privacy
  denial, and Finder/Preview opening remain manual tests on the generated Apple Silicon build.
