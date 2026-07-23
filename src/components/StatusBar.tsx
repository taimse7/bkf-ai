import type { BootstrapInfo, Repository, ScanProgress } from "../types";

interface Props {
  repositories: Repository[];
  scanProgress: ScanProgress | null;
  bootstrap: BootstrapInfo | null;
  message: string;
}

export function StatusBar({ repositories, scanProgress, bootstrap, message }: Props) {
  return (
    <footer className="status-bar">
      <span>{repositories.length} מאגרים</span>
      {scanProgress && (
        <span>
          {scanProgress.status}: {scanProgress.scanned.toLocaleString("he-IL")} קבצים
        </span>
      )}
      <span>{message}</span>
      {bootstrap && <span dir="ltr">API 127.0.0.1:{bootstrap.localApiPort}</span>}
    </footer>
  );
}
