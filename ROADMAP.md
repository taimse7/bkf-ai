# Roadmap

## שלב A — יסודות
- [x] מבנה Workspace.
- [x] מאגרים מרובים.
- [x] SQLite Catalog.
- [x] סריקה מצטברת.
- [x] API מקומי.
- [x] UI מפוצל Library/Viewer.
- [x] תוסף אוצריא ראשוני.
- [x] Tantivy foundation.
- [x] BKF Sidecar foundation.

## שלב B — אימות Native
- [ ] ריצת CI ירוקה על Apple Silicon.
- [ ] בניית `.app` ו־`.dmg`.
- [ ] בדיקת כונן חיצוני אמיתי עם מאות אלפי קבצים.
- [ ] בדיקת ניתוק כונן בזמן סריקה.
- [ ] בדיקת Disk Full והרשאות macOS.

## שלב C — Viewer
- [ ] Range Requests מלאים ב־API המקומי.
- [ ] הדפסה מלאה מתוך Viewer.
- [ ] Thumbnails.
- [ ] חיפוש בתוך PDF עם Text Layer.
- [ ] Cache eviction לפי LRU.

## שלב D — BKC
- [ ] הוספת Profiles מוכחים בלבד.
- [ ] אריזת מנוע Repair או החלפתו במימוש פנימי.
- [ ] Validator מלא יותר.
- [ ] Benchmark לספרים מעל 4GB.

## שלב E — BKF
- [ ] Page Boundary Profiles מוכחים.
- [ ] Sidecar capture/import workflow.
- [ ] DjVu renderer מורשה.
- [ ] Preview עמוד יחיד.
- [ ] Lazy loading.
- [ ] יצוא ספר מלא ל־PDF.
- [ ] Text Provider / OCR.

## שלב F — הפצה
- [ ] חתימת Developer ID.
- [ ] Notarization.
- [ ] Universal Binary.
- [ ] Installer משותף למנוע ולתוסף.
- [ ] Pairing אוטומטי בין התוסף למנוע.
