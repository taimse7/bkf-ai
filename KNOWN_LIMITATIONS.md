# Known Limitations

- This is a build proof, not a usable BKC/BKF conversion product.
- BKC reading and conversion are not implemented yet.
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
- The virtual library uses fixed-height rows and paged queries; search, sorting controls, and
  filtering are outside stage 2.
- This stage does not convert, decode, preview, or export BKC/BKF content.
- The first native macOS workflow exposed a missing Tauri icon and therefore did not reach app/DMG
  bundling. Icons are now included, but the corrected native workflow has not yet run.
