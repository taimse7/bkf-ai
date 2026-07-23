export type DocumentFormat = "BKC" | "BKF" | "PDF" | "Unknown";
export type SupportStatus =
  | "exact"
  | "repair"
  | "sidecar"
  | "renderer_required"
  | "unsupported"
  | "unknown";

export interface Repository {
  id: string;
  displayName: string;
  rootPath: string;
  connected: boolean;
  scanStatus: string;
  documentCount: number;
  indexedCount: number;
  lastScanMs: number | null;
}

export interface DocumentItem {
  id: string;
  repositoryId: string;
  repositoryName: string;
  name: string;
  relativePath: string;
  size: number;
  modifiedMs: number | null;
  format: DocumentFormat;
  status: string;
  supportStatus: SupportStatus;
  pageCount: number | null;
  textIndexed: boolean;
}

export interface DocumentPage {
  items: DocumentItem[];
  total: number;
  offset: number;
}

export interface ScanProgress {
  repositoryId: string;
  scanned: number;
  changed: number;
  errors: number;
  status: string;
  currentPath: string | null;
}

export interface SearchHit {
  repositoryId: string;
  documentId: string;
  documentName: string;
  repositoryName: string;
  pageIndex: number;
  snippet: string;
  score: number;
  textSource: string;
}

export interface PreviewDescriptor {
  kind: "pdf" | "bkf" | "unsupported";
  documentId: string;
  title: string;
  localPath: string | null;
  pageCount: number | null;
  message: string | null;
  supportStatus: SupportStatus;
}

export interface BootstrapInfo {
  appDataDir: string;
  localApiPort: number;
  localApiToken: string;
  localApiUrl: string;
}

export interface ViewerTab extends PreviewDescriptor {
  tabId: string;
  currentPage: number;
}
