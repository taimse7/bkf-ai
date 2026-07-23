# הוראות עדכון המאגר הקיים

ה־ZIP הזה מכיל פרויקט מלא חדש. מומלץ לעדכן באמצעות Branch ולא לדרוס מיד את `main`.

```bash
git clone https://github.com/taimse7/bkf-ai.git
cd bkf-ai
git checkout -b architecture/bkf-ai-next
```

חלץ את תוכן ה־ZIP אל תוך תיקיית המאגר ואשר החלפת קבצים.

לאחר מכן:

```bash
pnpm install
pnpm test
pnpm build

cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo test --manifest-path src-tauri/Cargo.toml --workspace

git add .
git commit -m "Refactor into BKF/BKC engine, search, viewer and Otzaria plugin"
git push -u origin architecture/bkf-ai-next
```

פתח Pull Request אל `main`.

## לפני Merge

יש לוודא שריצת GitHub Actions ירוקה ושנוצרו:

- `BKF AI.app`
- קובץ `.dmg`
- `otzaria-bkf-bkc.otzplugin`

אין למחוק את ה־Branch הישן עד לאחר בדיקה ידנית על Mac וכונן חיצוני.
