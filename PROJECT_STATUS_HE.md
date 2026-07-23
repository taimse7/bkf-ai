# מצב הפרויקט בחבילה זו

## ממומש בקוד

- אפליקציית Tauri 2 עם Rust, React ו־TypeScript.
- ממשק RTL עם ספרייה מימין ו־Viewer משמאל.
- מאגרים מרובים ב־SQLite.
- סריקה מצטברת מבוססת גודל וזמן שינוי.
- זיהוי BKC, BKF ו־PDF לפי תוכן.
- רשימה וירטואלית.
- תצוגת PDF באמצעות PDF.js.
- מצב מיקוד ומסך מלא.
- מנוע BKC עם פרופיל מדויק שנבדק ומסלול Repair מבוסס Ghostscript.
- תשתית Sidecar ל־200 הבתים הראשונים בכל עמוד BKF.
- Tantivy עם shard נפרד לכל מאגר.
- חילוץ טקסט מ־PDF ומ־BKF text sidecar.
- API מקומי ב־Rust עם token ו־Range Requests.
- תוסף אוצריא המציג מאגרים, רשימת קבצים, חיפוש ו־PDF.
- GitHub Actions לבדיקות ולבניית APP, DMG ותוסף.

## לא ממומש או לא מוכח

- אין מפענח BKF עצמאי וכללי.
- אין DjVu Renderer מחובר ולכן Sidecar לבדו עדיין אינו מציג BKF.
- אין יצוא BKF ל־PDF.
- אין OCR.
- אין חילוץ טקסט מכל וריאנט BKC/BKF.
- Ghostscript אינו מצורף ל־DMG.
- אין חתימה או notarization.
- אין בניית Intel/Universal.
- אין הוכחת קומפילציה מקומית בחבילה זו; נדרש CI על macOS.

אסור לתאר את החבילה כמוצר מוגמר או כממיר BKF מלא.
