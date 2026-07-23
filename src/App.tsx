import {
  BookOpen,
  Database,
  Moon,
  PanelRightClose,
  PanelRightOpen,
  Settings,
  Sun
} from "lucide-react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  addRepository,
  bootstrap,
  cancelScan,
  exportPdf,
  indexDocument,
  listDocuments,
  listRepositories,
  openLocalPath,
  preparePreview,
  scanRepository,
  searchLibrary
} from "./api";
import { useDebouncedValue } from "./hooks/useDebouncedValue";
import { LibraryPanel } from "./components/LibraryPanel";
import { ViewerPanel } from "./components/ViewerPanel";
import { StatusBar } from "./components/StatusBar";
import type {
  BootstrapInfo,
  DocumentItem,
  DocumentPage,
  Repository,
  ScanProgress,
  SearchHit,
  ViewerTab
} from "./types";

const EMPTY_PAGE: DocumentPage = { items: [], total: 0, offset: 0 };

export default function App() {
  const [bootstrapInfo, setBootstrapInfo] = useState<BootstrapInfo | null>(null);
  const [repositories, setRepositories] = useState<Repository[]>([]);
  const [selectedRepositoryIds, setSelectedRepositoryIds] = useState<string[]>([]);
  const [documentPage, setDocumentPage] = useState<DocumentPage>(EMPTY_PAGE);
  const [documents, setDocuments] = useState<Map<number, DocumentItem>>(new Map());
  const [libraryQuery, setLibraryQuery] = useState("");
  const [formatFilter, setFormatFilter] = useState("");
  const [mode, setMode] = useState<"library" | "search">("library");
  const [searchQuery, setSearchQuery] = useState("");
  const [searchHits, setSearchHits] = useState<SearchHit[]>([]);
  const [searchLoading, setSearchLoading] = useState(false);
  const [scanProgress, setScanProgress] = useState<ScanProgress | null>(null);
  const [tabs, setTabs] = useState<ViewerTab[]>([]);
  const [activeTabId, setActiveTabId] = useState<string | null>(null);
  const [focusMode, setFocusMode] = useState(false);
  const [zoom, setZoom] = useState(1);
  const [dark, setDark] = useState(false);
  const [message, setMessage] = useState("מוכן");
  const [libraryWidth, setLibraryWidth] = useState(390);
  const [resizing, setResizing] = useState(false);
  const loadedPages = useRef(new Set<number>());

  const debouncedLibraryQuery = useDebouncedValue(libraryQuery, 180);
  const debouncedSearchQuery = useDebouncedValue(searchQuery, 180);

  const refreshRepositories = useCallback(async () => {
    const rows = await listRepositories();
    setRepositories(rows);
    setSelectedRepositoryIds((current) => {
      const valid = current.filter((id) => rows.some((row) => row.id === id));
      return valid.length > 0 ? valid : rows.map((row) => row.id);
    });
  }, []);

  useEffect(() => {
    let active = true;
    void Promise.all([bootstrap(), listRepositories()])
      .then(([info, rows]) => {
        if (!active) return;
        setBootstrapInfo(info);
        setRepositories(rows);
        setSelectedRepositoryIds(rows.map((row) => row.id));
      })
      .catch((reason: unknown) => setMessage(`שגיאת אתחול: ${String(reason)}`));

    const unlisten = listen<ScanProgress>("repository-scan-progress", ({ payload }) => {
      setScanProgress(payload);
      if (payload.status !== "running") {
        loadedPages.current.clear();
        void refreshRepositories();
      }
    });

    return () => {
      active = false;
      void unlisten.then((dispose) => dispose());
    };
  }, [refreshRepositories]);

  const loadDocumentPage = useCallback(async (offset: number, force = false) => {
    const pageOffset = Math.floor(offset / 200) * 200;
    if (!force && loadedPages.current.has(pageOffset)) return;
    loadedPages.current.add(pageOffset);

    try {
      const page = await listDocuments({
        repositoryIds: selectedRepositoryIds,
        query: debouncedLibraryQuery,
        format: formatFilter,
        offset: pageOffset,
        limit: 200
      });
      setDocumentPage(page);
      setDocuments((current) => {
        const next = new Map(current);
        page.items.forEach((item, index) => next.set(page.offset + index, item));
        return next;
      });
    } catch (reason) {
      loadedPages.current.delete(pageOffset);
      setMessage(`טעינת הספרייה נכשלה: ${String(reason)}`);
    }
  }, [selectedRepositoryIds, debouncedLibraryQuery, formatFilter]);

  useEffect(() => {
    loadedPages.current.clear();
    setDocuments(new Map());
    setDocumentPage(EMPTY_PAGE);
    if (selectedRepositoryIds.length > 0) void loadDocumentPage(0, true);
  }, [selectedRepositoryIds, debouncedLibraryQuery, formatFilter, loadDocumentPage]);

  useEffect(() => {
    if (
      scanProgress &&
      scanProgress.status !== "running" &&
      selectedRepositoryIds.includes(scanProgress.repositoryId)
    ) {
      loadedPages.current.clear();
      void loadDocumentPage(0, true);
    }
  }, [scanProgress?.status, scanProgress?.repositoryId, selectedRepositoryIds, loadDocumentPage]);

  useEffect(() => {
    if (!debouncedSearchQuery.trim() || selectedRepositoryIds.length === 0) {
      setSearchHits([]);
      setSearchLoading(false);
      return;
    }
    let active = true;
    setSearchLoading(true);
    void searchLibrary({
      query: debouncedSearchQuery,
      repositoryIds: selectedRepositoryIds,
      limit: 100
    })
      .then((hits) => active && setSearchHits(hits))
      .catch((reason: unknown) => active && setMessage(`החיפוש נכשל: ${String(reason)}`))
      .finally(() => active && setSearchLoading(false));
    return () => {
      active = false;
    };
  }, [debouncedSearchQuery, selectedRepositoryIds]);

  useEffect(() => {
    if (!resizing) return;
    const move = (event: MouseEvent) => {
      const width = Math.min(620, Math.max(320, window.innerWidth - event.clientX));
      setLibraryWidth(width);
    };
    const stop = () => setResizing(false);
    window.addEventListener("mousemove", move);
    window.addEventListener("mouseup", stop);
    return () => {
      window.removeEventListener("mousemove", move);
      window.removeEventListener("mouseup", stop);
    };
  }, [resizing]);

  const handleAddRepository = async () => {
    const selected = await open({
      directory: true,
      multiple: false,
      title: "בחירת מאגר BKC/BKF"
    });
    if (!selected || Array.isArray(selected)) return;
    try {
      const repository = await addRepository(selected);
      await refreshRepositories();
      setSelectedRepositoryIds((current) => [...new Set([...current, repository.id])]);
      await scanRepository(repository.id);
      setMessage("המאגר נוסף והסריקה התחילה");
    } catch (reason) {
      setMessage(`הוספת המאגר נכשלה: ${String(reason)}`);
    }
  };

  const handleOpenDocument = async (document: DocumentItem, page = 1) => {
    setMessage(`מכין תצוגה: ${document.name}`);
    try {
      const preview = await preparePreview(document.id);
      const existing = tabs.find((tab) => tab.documentId === document.id);
      if (existing) {
        setTabs((current) =>
          current.map((tab) => tab.tabId === existing.tabId
            ? { ...tab, ...preview, currentPage: Math.max(1, page) }
            : tab)
        );
        setActiveTabId(existing.tabId);
      } else {
        const tab: ViewerTab = {
          ...preview,
          tabId: crypto.randomUUID(),
          currentPage: Math.max(1, page)
        };
        setTabs((current) => [...current, tab]);
        setActiveTabId(tab.tabId);
      }
      setMessage(preview.message ?? "המסמך מוכן");
    } catch (reason) {
      setMessage(`פתיחת המסמך נכשלה: ${String(reason)}`);
    }
  };

  const handleOpenSearchHit = async (hit: SearchHit) => {
    const document = [...documents.values()].find((item) => item.id === hit.documentId);
    if (document) {
      await handleOpenDocument(document, hit.pageIndex + 1);
      return;
    }
    const page = await listDocuments({
      repositoryIds: [hit.repositoryId],
      query: hit.documentName,
      format: "",
      offset: 0,
      limit: 20
    });
    const found = page.items.find((item) => item.id === hit.documentId);
    if (found) await handleOpenDocument(found, hit.pageIndex + 1);
  };

  const handleExportDocument = async (document: DocumentItem) => {
    const target = await save({
      title: "יצוא ל־PDF",
      defaultPath: `${document.name.replace(/\.[^.]+$/, "")}.pdf`
    });
    if (!target) return;
    try {
      await exportPdf(document.id, target);
      setMessage("ה־PDF נשמר בהצלחה");
    } catch (reason) {
      setMessage(`היצוא נכשל: ${String(reason)}`);
    }
  };

  const handleExportTab = async (tab: ViewerTab) => {
    const document = [...documents.values()].find((item) => item.id === tab.documentId);
    if (document) await handleExportDocument(document);
  };

  const handleIndexDocument = async (document: DocumentItem) => {
    setMessage(`מאנדקס טקסט: ${document.name}`);
    try {
      const pages = await indexDocument(document.id);
      setMessage(`${pages.toLocaleString("he-IL")} עמודים נוספו לאינדקס`);
      loadedPages.current.clear();
      await loadDocumentPage(0, true);
      if (searchQuery.trim()) {
        setSearchHits(await searchLibrary({
          query: searchQuery,
          repositoryIds: selectedRepositoryIds,
          limit: 100
        }));
      }
    } catch (reason) {
      setMessage(`אינדוקס הטקסט נכשל: ${String(reason)}`);
    }
  };

  const activeTab = useMemo(
    () => tabs.find((tab) => tab.tabId === activeTabId) ?? null,
    [tabs, activeTabId]
  );

  const closeTab = (id: string) => {
    setTabs((current) => {
      const index = current.findIndex((tab) => tab.tabId === id);
      const next = current.filter((tab) => tab.tabId !== id);
      if (activeTabId === id) {
        setActiveTabId(next[Math.max(0, index - 1)]?.tabId ?? null);
      }
      return next;
    });
  };

  const toggleFullscreen = async () => {
    const window = getCurrentWindow();
    await window.setFullscreen(!(await window.isFullscreen()));
  };

  return (
    <main className={`app ${dark ? "theme-dark" : ""} ${focusMode ? "focus-mode" : ""}`}>
      <header className="top-bar">
        <div className="brand">
          <BookOpen size={22} />
          <strong>BKF AI</strong>
          <span>ספריית BKC/BKF</span>
        </div>

        <div className="top-actions">
          <button title="מאגרים" onClick={handleAddRepository}>
            <Database size={17} />
            הוסף מאגר
          </button>
          <button title="הצג או הסתר ספרייה" onClick={() => setFocusMode((value) => !value)}>
            {focusMode ? <PanelRightOpen size={17} /> : <PanelRightClose size={17} />}
          </button>
          <button title="מצב בהיר או כהה" onClick={() => setDark((value) => !value)}>
            {dark ? <Sun size={17} /> : <Moon size={17} />}
          </button>
          <button title="הגדרות">
            <Settings size={17} />
          </button>
        </div>
      </header>

      <div className="workspace">
        <ViewerPanel
          tabs={tabs}
          activeTabId={activeTabId}
          focusMode={focusMode}
          zoom={zoom}
          onActiveTab={setActiveTabId}
          onCloseTab={closeTab}
          onToggleFocus={() => setFocusMode((value) => !value)}
          onToggleFullscreen={() => void toggleFullscreen()}
          onZoomChange={setZoom}
          onPageChange={(tabId, page) =>
            setTabs((current) => current.map((tab) => tab.tabId === tabId ? { ...tab, currentPage: page } : tab))
          }
          onPageCount={(tabId, count) =>
            setTabs((current) => current.map((tab) => tab.tabId === tabId ? { ...tab, pageCount: count } : tab))
          }
          onExport={(tab) => void handleExportTab(tab)}
          onOpenPath={(path) => void openLocalPath(path)}
        />

        {!focusMode && (
          <>
            <div className="splitter" onMouseDown={() => setResizing(true)} />
            <div style={{ width: libraryWidth, minWidth: libraryWidth }}>
              <LibraryPanel
                repositories={repositories}
                selectedRepositoryIds={selectedRepositoryIds}
                onRepositorySelectionChange={setSelectedRepositoryIds}
                onAddRepository={() => void handleAddRepository()}
                onScanRepository={(id) => void scanRepository(id)}
                documents={documents}
                documentPage={documentPage}
                onLoadDocumentPage={(offset) => void loadDocumentPage(offset)}
                libraryQuery={libraryQuery}
                onLibraryQueryChange={setLibraryQuery}
                formatFilter={formatFilter}
                onFormatFilterChange={setFormatFilter}
                onOpenDocument={(document, page) => void handleOpenDocument(document, page)}
                onExportDocument={(document) => void handleExportDocument(document)}
                onIndexDocument={(document) => void handleIndexDocument(document)}
                searchQuery={searchQuery}
                onSearchQueryChange={setSearchQuery}
                searchHits={searchHits}
                searchLoading={searchLoading}
                onOpenSearchHit={(hit) => void handleOpenSearchHit(hit)}
                mode={mode}
                onModeChange={setMode}
              />
            </div>
          </>
        )}
      </div>

      {!focusMode && (
        <StatusBar
          repositories={repositories}
          scanProgress={scanProgress}
          bootstrap={bootstrapInfo}
          message={message}
        />
      )}
    </main>
  );
}
