# בנייה ובדיקה על macOS

## דרישות פיתוח

- macOS 14 ומעלה מומלץ
- Apple Silicon
- Node.js 24
- pnpm 11.7
- Rust stable
- Xcode Command Line Tools

## בדיקות

```bash
pnpm install --no-frozen-lockfile
pnpm verify
pnpm test
pnpm build
pnpm plugin:build
pnpm plugin:package
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --workspace --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml --workspace
```

## בניית האפליקציה

```bash
pnpm tauri build --target aarch64-apple-darwin --bundles app,dmg
```

התוצרים אמורים להופיע תחת:

```text
src-tauri/target/aarch64-apple-darwin/release/bundle/macos/
src-tauri/target/aarch64-apple-darwin/release/bundle/dmg/
```

## בניית תוסף אוצריא

```bash
pnpm plugin:build
pnpm plugin:package
```

התוצר:

```text
plugins/otzaria-bkf-bkc/otzaria-bkf-bkc.otzplugin
```

## בדיקת קבלה מינימלית

1. הוסף מאגר מקומי.
2. סרוק אותו ובדוק שהיישום נשאר מגיב.
3. פתח PDF רגיל.
4. פתח BKC בעל פרופיל מאומת.
5. ודא ש־BKF בלי Sidecar אינו מסומן כנתמך.
6. בנה אינדקס טקסט למסמך PDF ובצע חיפוש.
7. עבור למצב מיקוד ולמסך מלא.
8. בצע יצוא PDF לתיקייה שאינה תיקיית המקור.
9. בנה והתקן את תוסף אוצריא ובדוק חיבור לשרת המקומי.

## מגבלת אימות

חבילת המקור נוצרה בסביבה שאין בה Rust toolchain או macOS SDK. בדיקות מבנה, JSON וסינטקס TypeScript בוצעו, אך רק ריצת GitHub Actions ירוקה או בנייה מקומית על Mac מוכיחות שהקוד כולו מתקמפל ונארז.
