import { Database, HardDrive, Plus, RefreshCw } from "lucide-react";
import type { Repository } from "../types";

interface Props {
  repositories: Repository[];
  selectedIds: string[];
  onChange: (ids: string[]) => void;
  onAdd: () => void;
  onScan: (repositoryId: string) => void;
}

export function RepositorySelector({
  repositories,
  selectedIds,
  onChange,
  onAdd,
  onScan
}: Props) {
  const allSelected = repositories.length > 0 && selectedIds.length === repositories.length;

  const toggle = (id: string) => {
    onChange(
      selectedIds.includes(id)
        ? selectedIds.filter((value) => value !== id)
        : [...selectedIds, id]
    );
  };

  return (
    <details className="repository-selector">
      <summary>
        <Database size={16} />
        <span>
          {allSelected
            ? "כל המאגרים"
            : selectedIds.length === 1
              ? repositories.find((item) => item.id === selectedIds[0])?.displayName ?? "מאגר אחד"
              : `${selectedIds.length} מאגרים נבחרו`}
        </span>
      </summary>

      <div className="repository-popover">
        <div className="repository-popover-actions">
          <button onClick={() => onChange(repositories.map((item) => item.id))}>בחר הכול</button>
          <button onClick={() => onChange([])}>נקה</button>
        </div>

        <div className="repository-options">
          {repositories.map((repository) => (
            <div className="repository-option" key={repository.id}>
              <label>
                <input
                  type="checkbox"
                  checked={selectedIds.includes(repository.id)}
                  onChange={() => toggle(repository.id)}
                />
                <HardDrive size={15} />
                <span className="repository-name">{repository.displayName}</span>
                <small>{repository.documentCount.toLocaleString("he-IL")} קבצים</small>
                <span
                  className={`connection-dot ${repository.connected ? "connected" : "disconnected"}`}
                  title={repository.connected ? "מחובר" : "מנותק"}
                />
              </label>
              <button
                className="icon-button"
                title="סריקה מחדש"
                onClick={() => onScan(repository.id)}
              >
                <RefreshCw size={14} />
              </button>
            </div>
          ))}
        </div>

        <button className="add-repository-button" onClick={onAdd}>
          <Plus size={15} />
          הוסף מאגר
        </button>
      </div>
    </details>
  );
}
