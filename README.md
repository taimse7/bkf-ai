# BKF AI Next

אפליקציית macOS ותוסף אוצריא לניהול מאגרי BKC/BKF, סריקה מהירה, חיפוש מקומי,
תצוגה, הדפסה ויצוא ל־PDF.

## מה כלול

- Tauri 2 + Rust + React + TypeScript.
- מאגרים מרובים עם SQLite.
- סריקה מצטברת בקריאה בלבד.
- מנוע BKC עם פרופיל Golden מאומת ונתיב Repair אופציונלי.
- תשתית BKF עם Sidecar ל־200 הבתים הראשונים בכל עמוד.
- Tantivy לחיפוש טקסט מקומי.
- API מקומי ב־Rust על `127.0.0.1`.
- תוסף אוצריא שמתחבר ל־API המקומי.
- Viewer ל־PDF, מצב מיקוד ומסך מלא.
- תור משימות, Cache, אבחון ו־CI ל־Apple Silicon.

## חשוב

הפרויקט אינו טוען שקיים מפענח BKF כללי. BKF דורש:

1. Sidecar תואם המכיל prefix מפוענח לכל עמוד; או
2. Decoder מאומת שימומש בעתיד.

גם BKC אינו נתמך אוטומטית בכל וריאנט. המסלול המדויק קיים לפרופיל המאומת,
ומסלול Repair דורש Ghostscript זמין.

## התחלה

```bash
pnpm install
pnpm test
pnpm build
pnpm tauri dev
```

בדיקות Rust:

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo test --manifest-path src-tauri/Cargo.toml --workspace
```

## בניית macOS

```bash
pnpm tauri build --target aarch64-apple-darwin --bundles app,dmg
```

## תוסף אוצריא

```bash
pnpm plugin:build
```

הפלט נמצא תחת:

```text
plugins/otzaria-bkf-bkc/dist
```

למידע מלא:

- `ARCHITECTURE.md`
- `ROADMAP.md`
- `KNOWN_LIMITATIONS.md`
- `UPDATE-INSTRUCTIONS.md`
