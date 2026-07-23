import {
  ChevronLeft,
  ChevronRight,
  Download,
  Expand,
  Focus,
  FolderOpen,
  Minus,
  Plus,
  Printer,
  X
} from "lucide-react";
import { convertFileSrc } from "@tauri-apps/api/core";
import type { ViewerTab } from "../types";
import { PdfViewer } from "./PdfViewer";

interface Props {
  tabs: ViewerTab[];
  activeTabId: string | null;
  focusMode: boolean;
  zoom: number;
  onActiveTab: (id: string) => void;
  onCloseTab: (id: string) => void;
  onToggleFocus: () => void;
  onToggleFullscreen: () => void;
  onZoomChange: (zoom: number) => void;
  onPageChange: (tabId: string, page: number) => void;
  onPageCount: (tabId: string, count: number) => void;
  onExport: (tab: ViewerTab) => void;
  onOpenPath: (path: string) => void;
}

export function ViewerPanel(props: Props) {
  const active = props.tabs.find((tab) => tab.tabId === props.activeTabId) ?? null;

  return (
    <section className="viewer-panel">
      <div className="document-tabs">
        {props.tabs.map((tab) => (
          <button
            key={tab.tabId}
            className={tab.tabId === props.activeTabId ? "active" : ""}
            onClick={() => props.onActiveTab(tab.tabId)}
          >
            <span>{tab.title}</span>
            <X
              size={13}
              onClick={(event) => {
                event.stopPropagation();
                props.onCloseTab(tab.tabId);
              }}
            />
          </button>
        ))}
      </div>

      <div className="viewer-toolbar">
        <div className="toolbar-group">
          <button title="מצב מיקוד" onClick={props.onToggleFocus}>
            <Focus size={16} />
          </button>
          <button title="מסך מלא" onClick={props.onToggleFullscreen}>
            <Expand size={16} />
          </button>
          {active?.localPath && (
            <button title="פתיחה ב־Preview" onClick={() => props.onOpenPath(active.localPath!)}>
              <FolderOpen size={16} />
            </button>
          )}
        </div>

        <div className="toolbar-group page-controls">
          <button
            title="עמוד קודם"
            disabled={!active || active.currentPage <= 1}
            onClick={() => active && props.onPageChange(active.tabId, active.currentPage - 1)}
          >
            <ChevronRight size={16} />
          </button>
          <span>
            {active?.currentPage ?? 0} / {active?.pageCount ?? 0}
          </span>
          <button
            title="עמוד הבא"
            disabled={!active || active.pageCount == null || active.currentPage >= active.pageCount}
            onClick={() => active && props.onPageChange(active.tabId, active.currentPage + 1)}
          >
            <ChevronLeft size={16} />
          </button>
        </div>

        <div className="toolbar-group">
          <button title="הקטנת תצוגה" onClick={() => props.onZoomChange(Math.max(0.35, props.zoom - 0.1))}>
            <Minus size={16} />
          </button>
          <span>{Math.round(props.zoom * 100)}%</span>
          <button title="הגדלת תצוגה" onClick={() => props.onZoomChange(Math.min(3, props.zoom + 0.1))}>
            <Plus size={16} />
          </button>
        </div>

        <div className="toolbar-group">
          <button title="הדפסה דרך Preview" disabled={!active?.localPath} onClick={() => active?.localPath && props.onOpenPath(active.localPath)}>
            <Printer size={16} />
          </button>
          <button title="יצוא ל־PDF" disabled={!active} onClick={() => active && props.onExport(active)}>
            <Download size={16} />
          </button>
        </div>
      </div>

      <div className="viewer-canvas">
        {!active ? (
          <div className="viewer-empty">
            <strong>בחר ספר מהספרייה</strong>
            <span>לחיצה אחת מציגה Preview. לחיצה כפולה משאירה לשונית פתוחה.</span>
          </div>
        ) : active.kind === "pdf" && active.localPath ? (
          <PdfViewer
            source={convertFileSrc(active.localPath)}
            page={active.currentPage}
            onPageChange={(page) => props.onPageChange(active.tabId, page)}
            onPageCount={(count) => props.onPageCount(active.tabId, count)}
            zoom={props.zoom}
          />
        ) : active.kind === "bkf" ? (
          <div className="viewer-empty warning">
            <strong>BKF זוהה</strong>
            <span>{active.message ?? "נדרש Sidecar ו־DjVu Renderer מתאים."}</span>
          </div>
        ) : (
          <div className="viewer-empty warning">
            <strong>הקובץ אינו זמין לתצוגה</strong>
            <span>{active.message}</span>
          </div>
        )}
      </div>
    </section>
  );
}
