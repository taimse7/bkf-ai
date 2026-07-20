import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { visibleRange } from "./virtualization";

type FileType = "BKC" | "BKF" | "Unknown";

interface LibraryItem {
  name: string;
  relativePath: string;
  size: number;
  fileType: FileType;
  modifiedMs: number | null;
  status: string;
  selected: boolean;
}

interface LibraryPage {
  items: LibraryItem[];
  total: number;
  offset: number;
}

interface ScanRun {
  id: string;
  rootPath: string;
  status: string;
  scanned: number;
  errors: number;
  generation: number;
}

interface ScanProgress {
  scanId: string;
  status: string;
  scanned: number;
  errors: number;
  currentPath: string | null;
}

const ROW_HEIGHT = 58;
const PAGE_SIZE = 240;

const statusLabels: Record<string, string> = {
  running: "סורק",
  completed: "הושלם",
  completed_with_errors: "הושלם עם שגיאות",
  cancelled: "בוטל",
  disconnected: "הכונן נותק",
  permission_denied: "אין הרשאה",
  read_error: "שגיאת קריאה",
  ready: "מוכן",
};

function formatSize(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let value = bytes / 1024;
  let unit = units[0];
  for (let index = 1; value >= 1024 && index < units.length; index += 1) {
    value /= 1024;
    unit = units[index];
  }
  return `${value.toLocaleString("he-IL", { maximumFractionDigits: 1 })} ${unit}`;
}

function App() {
  const [run, setRun] = useState<ScanRun | null>(null);
  const [items, setItems] = useState<Map<number, LibraryItem>>(new Map());
  const [total, setTotal] = useState(0);
  const [scrollTop, setScrollTop] = useState(0);
  const [viewportHeight, setViewportHeight] = useState(520);
  const [busy, setBusy] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const viewportRef = useRef<HTMLDivElement>(null);
  const loadedPages = useRef(new Set<number>());

  const loadPage = useCallback(async (scanId: string, offset: number, force = false) => {
    const pageOffset = Math.floor(offset / PAGE_SIZE) * PAGE_SIZE;
    if (!force && loadedPages.current.has(pageOffset)) return;
    loadedPages.current.add(pageOffset);
    try {
      const page = await invoke<LibraryPage>("get_library_page", {
        scanId,
        offset: pageOffset,
        limit: PAGE_SIZE,
      });
      setTotal(page.total);
      setItems((current) => {
        const next = new Map(current);
        page.items.forEach((item, index) => next.set(page.offset + index, item));
        return next;
      });
    } catch (reason) {
      loadedPages.current.delete(pageOffset);
      setError(String(reason));
    }
  }, []);

  const resetLibrary = useCallback((scan: ScanRun) => {
    setRun(scan);
    setItems(new Map());
    setTotal(0);
    loadedPages.current.clear();
    void loadPage(scan.id, 0, true);
  }, [loadPage]);

  useEffect(() => {
    let active = true;
    const initialise = async () => {
      try {
        const scan = await invoke<ScanRun | null>("resume_last_scan");
        if (active && scan) resetLibrary(scan);
      } catch (reason) {
        if (active) setError(String(reason));
      } finally {
        if (active) setBusy(false);
      }
    };
    void initialise();
    const unlisten = listen<ScanProgress>("scan-progress", ({ payload }) => {
      setRun((current) => current?.id === payload.scanId ? {
        ...current,
        status: payload.status,
        scanned: payload.scanned,
        errors: payload.errors,
      } : current);
      if (payload.scanned % 1000 === 0 || payload.status !== "running") {
        loadedPages.current.clear();
        setRun((current) => {
          if (current?.id === payload.scanId) void loadPage(current.id, 0, true);
          return current;
        });
      }
    });
    return () => {
      active = false;
      void unlisten.then((dispose) => dispose());
    };
  }, [loadPage, resetLibrary]);

  useEffect(() => {
    const viewport = viewportRef.current;
    if (!viewport) return;
    const observer = new ResizeObserver(([entry]) => setViewportHeight(entry.contentRect.height));
    observer.observe(viewport);
    return () => observer.disconnect();
  }, []);

  const range = useMemo(
    () => visibleRange(total, ROW_HEIGHT, scrollTop, viewportHeight, 8),
    [total, scrollTop, viewportHeight],
  );

  useEffect(() => {
    if (run && total > 0) void loadPage(run.id, range.start);
  }, [loadPage, range.start, run, total]);

  const chooseSource = async () => {
    setError(null);
    const selected = await open({ directory: true, multiple: false, title: "בחירת תיקיית מקור או כונן" });
    if (!selected || Array.isArray(selected)) return;
    setBusy(true);
    try {
      const scan = await invoke<ScanRun>("start_scan", { sourcePath: selected });
      resetLibrary(scan);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(false);
    }
  };

  const cancel = async () => {
    if (run) await invoke("cancel_scan", { scanId: run.id });
  };

  const toggleSelected = async (index: number, item: LibraryItem) => {
    if (!run) return;
    const selected = !item.selected;
    setItems((current) => new Map(current).set(index, { ...item, selected }));
    try {
      await invoke("update_selected", {
        scanId: run.id,
        relativePath: item.relativePath,
        selected,
      });
    } catch (reason) {
      setItems((current) => new Map(current).set(index, item));
      setError(String(reason));
    }
  };

  const visibleRows = [];
  for (let index = range.start; index < range.end; index += 1) {
    const item = items.get(index);
    visibleRows.push(
      <div className="library-row" style={{ transform: `translateY(${index * ROW_HEIGHT}px)` }} key={index}>
        {item ? (
          <>
            <label className="check-cell" aria-label={`סימון ${item.name}`}>
              <input type="checkbox" checked={item.selected} onChange={() => void toggleSelected(index, item)} />
            </label>
            <strong className="ellipsis" title={item.name}>{item.name}</strong>
            <span className="ellipsis path" title={item.relativePath} dir="auto">{item.relativePath}</span>
            <span dir="ltr">{formatSize(item.size)}</span>
            <span className={`type type-${item.fileType.toLowerCase()}`}>{item.fileType}</span>
            <time>{item.modifiedMs ? new Date(item.modifiedMs).toLocaleString("he-IL") : "—"}</time>
            <span className={`item-status status-${item.status}`}>{statusLabels[item.status] ?? item.status}</span>
          </>
        ) : <span className="row-loading">טוען רשומה…</span>}
      </div>,
    );
  }

  const isRunning = run?.status === "running";

  return (
    <main className="app-shell">
      <header className="topbar">
        <div>
          <p className="eyebrow">Scanner and Library UI</p>
          <h1>ספריית BKF AI</h1>
        </div>
        <div className="actions">
          {isRunning && <button className="secondary danger" onClick={() => void cancel()}>ביטול סריקה</button>}
          <button className="primary" onClick={() => void chooseSource()} disabled={busy || isRunning}>
            בחירת תיקייה או כונן
          </button>
        </div>
      </header>

      <section className="source-card" aria-live="polite">
        <div>
          <span className="label">מקור לקריאה בלבד</span>
          <strong className="source-path" dir="auto">{run?.rootPath ?? "טרם נבחר מקור"}</strong>
        </div>
        <div className="scan-metrics">
          <span>{statusLabels[run?.status ?? ""] ?? (busy ? "טוען" : "ממתין")}</span>
          <strong>{(run?.scanned ?? 0).toLocaleString("he-IL")} קבצים</strong>
          {(run?.errors ?? 0) > 0 && <span className="error-count">{run?.errors} שגיאות</span>}
        </div>
      </section>

      {error && <div className="error-banner" role="alert">{error}</div>}

      <section className="library-card" aria-label="רשימת קבצים">
        <div className="library-header">
          <span>סימון</span><span>שם</span><span>נתיב יחסי</span><span>גודל</span>
          <span>סוג</span><span>תאריך שינוי</span><span>סטטוס</span>
        </div>
        <div
          className="virtual-viewport"
          ref={viewportRef}
          onScroll={(event) => setScrollTop(event.currentTarget.scrollTop)}
        >
          {total === 0 && !isRunning ? (
            <div className="empty-state">בחר תיקייה או כונן כדי לבנות את הספרייה.</div>
          ) : (
            <div className="virtual-spacer" style={{ height: total * ROW_HEIGHT }}>{visibleRows}</div>
          )}
        </div>
        <footer className="library-footer">
          <span>{total.toLocaleString("he-IL")} רשומות במסד הנתונים</span>
          <span>מוצגות רק השורות שבחלון — הרשימה אינה נטענת כולה לזיכרון</span>
        </footer>
      </section>
    </main>
  );
}

export default App;
