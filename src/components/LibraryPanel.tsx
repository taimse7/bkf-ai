import {
  FileDown,
  FileSearch,
  FolderOpen,
  Play,
  Search,
  TextSearch
} from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import type { DocumentItem, DocumentPage, Repository, SearchHit } from "../types";
import { visibleRange } from "../virtualization";
import { RepositorySelector } from "./RepositorySelector";
import { SearchResults } from "./SearchResults";

const ROW_HEIGHT = 46;
const PAGE_SIZE = 200;

interface Props {
  repositories: Repository[];
  selectedRepositoryIds: string[];
  onRepositorySelectionChange: (ids: string[]) => void;
  onAddRepository: () => void;
  onScanRepository: (id: string) => void;
  documents: Map<number, DocumentItem>;
  documentPage: DocumentPage;
  onLoadDocumentPage: (offset: number) => void;
  libraryQuery: string;
  onLibraryQueryChange: (value: string) => void;
  formatFilter: string;
  onFormatFilterChange: (value: string) => void;
  onOpenDocument: (document: DocumentItem, page?: number) => void;
  onExportDocument: (document: DocumentItem) => void;
  onIndexDocument: (document: DocumentItem) => void;
  searchQuery: string;
  onSearchQueryChange: (value: string) => void;
  searchHits: SearchHit[];
  searchLoading: boolean;
  onOpenSearchHit: (hit: SearchHit) => void;
  mode: "library" | "search";
  onModeChange: (mode: "library" | "search") => void;
}

export function LibraryPanel(props: Props) {
  const viewportRef = useRef<HTMLDivElement>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const [height, setHeight] = useState(600);

  useEffect(() => {
    const element = viewportRef.current;
    if (!element) return;
    const observer = new ResizeObserver(([entry]) => setHeight(entry.contentRect.height));
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  const range = useMemo(
    () => visibleRange(props.documentPage.total, ROW_HEIGHT, scrollTop, height, 10),
    [props.documentPage.total, scrollTop, height]
  );

  useEffect(() => {
    if (props.mode === "library" && props.documentPage.total > 0) {
      props.onLoadDocumentPage(Math.floor(range.start / PAGE_SIZE) * PAGE_SIZE);
    }
  }, [props.mode, props.documentPage.total, range.start, props.onLoadDocumentPage]);

  const rows = [];
  for (let index = range.start; index < range.end; index += 1) {
    const document = props.documents.get(index);
    rows.push(
      <div
        className="document-row"
        style={{ transform: `translateY(${index * ROW_HEIGHT}px)` }}
        key={document?.id ?? index}
        onDoubleClick={() => document && props.onOpenDocument(document)}
      >
        {document ? (
          <>
            <span className={`format-badge format-${document.format.toLowerCase()}`}>
              {document.format}
            </span>
            <button className="document-name" onClick={() => props.onOpenDocument(document)}>
              {document.name}
            </button>
            <span className="document-repository">{document.repositoryName}</span>
            <span className="document-support">{supportLabel(document)}</span>
            <div className="document-actions">
              <button title="פתיחה" onClick={() => props.onOpenDocument(document)}>
                <Play size={14} />
              </button>
              <button title="אינדוקס טקסט" onClick={() => props.onIndexDocument(document)}>
                <TextSearch size={14} />
              </button>
              <button title="יצוא ל־PDF" onClick={() => props.onExportDocument(document)}>
                <FileDown size={14} />
              </button>
            </div>
          </>
        ) : (
          <span className="row-placeholder">טוען…</span>
        )}
      </div>
    );
  }

  return (
    <aside className="library-panel">
      <div className="panel-tabs">
        <button
          className={props.mode === "library" ? "active" : ""}
          onClick={() => props.onModeChange("library")}
        >
          <FolderOpen size={15} />
          ספרייה
        </button>
        <button
          className={props.mode === "search" ? "active" : ""}
          onClick={() => props.onModeChange("search")}
        >
          <FileSearch size={15} />
          חיפוש בטקסט
        </button>
      </div>

      <RepositorySelector
        repositories={props.repositories}
        selectedIds={props.selectedRepositoryIds}
        onChange={props.onRepositorySelectionChange}
        onAdd={props.onAddRepository}
        onScan={props.onScanRepository}
      />

      {props.mode === "library" ? (
        <>
          <div className="library-filters">
            <label className="search-input">
              <Search size={16} />
              <input
                value={props.libraryQuery}
                onChange={(event) => props.onLibraryQueryChange(event.target.value)}
                placeholder="חיפוש לפי שם קובץ"
              />
            </label>
            <select
              value={props.formatFilter}
              onChange={(event) => props.onFormatFilterChange(event.target.value)}
            >
              <option value="">כל הסוגים</option>
              <option value="BKC">BKC</option>
              <option value="BKF">BKF</option>
              <option value="PDF">PDF</option>
              <option value="Unknown">לא מזוהה</option>
            </select>
          </div>

          <div className="document-header">
            <span>סוג</span>
            <span>שם</span>
            <span>מאגר</span>
            <span>מצב</span>
            <span />
          </div>

          <div
            className="document-viewport"
            ref={viewportRef}
            onScroll={(event) => setScrollTop(event.currentTarget.scrollTop)}
          >
            {props.documentPage.total === 0 ? (
              <div className="panel-state">לא נמצאו מסמכים במאגרים שנבחרו.</div>
            ) : (
              <div
                className="virtual-spacer"
                style={{ height: props.documentPage.total * ROW_HEIGHT }}
              >
                {rows}
              </div>
            )}
          </div>

          <footer className="panel-footer">
            {props.documentPage.total.toLocaleString("he-IL")} מסמכים
          </footer>
        </>
      ) : (
        <>
          <label className="global-search-input">
            <Search size={18} />
            <input
              value={props.searchQuery}
              onChange={(event) => props.onSearchQueryChange(event.target.value)}
              placeholder="חיפוש בכל הספרייה"
              autoFocus
            />
          </label>
          <SearchResults
            hits={props.searchHits}
            loading={props.searchLoading}
            query={props.searchQuery}
            onOpen={props.onOpenSearchHit}
          />
        </>
      )}
    </aside>
  );
}

function supportLabel(document: DocumentItem) {
  switch (document.supportStatus) {
    case "exact":
      return "שחזור מדויק";
    case "repair":
      return "נדרש Repair";
    case "sidecar":
      return "Sidecar נמצא";
    case "renderer_required":
      return "נדרש Renderer";
    case "unsupported":
      return "לא נתמך";
    default:
      return document.status;
  }
}
