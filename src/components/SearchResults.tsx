import { FileSearch, SearchX } from "lucide-react";
import type { SearchHit } from "../types";

interface Props {
  hits: SearchHit[];
  loading: boolean;
  query: string;
  onOpen: (hit: SearchHit) => void;
}

export function SearchResults({ hits, loading, query, onOpen }: Props) {
  if (loading) {
    return <div className="panel-state">מחפש באינדקס המקומי…</div>;
  }

  if (!query.trim()) {
    return (
      <div className="panel-state">
        <FileSearch size={34} />
        <strong>חיפוש בטקסט בכל הספרייה</strong>
        <span>הקלד מילה או ביטוי. החיפוש מתבצע באינדקס Tantivy המקומי.</span>
      </div>
    );
  }

  if (hits.length === 0) {
    return (
      <div className="panel-state">
        <SearchX size={34} />
        <strong>לא נמצאו תוצאות</strong>
        <span>ייתכן שהמסמכים טרם עברו אינדוקס טקסט.</span>
      </div>
    );
  }

  return (
    <div className="search-results">
      {hits.map((hit, index) => (
        <button
          className="search-hit"
          key={`${hit.documentId}-${hit.pageIndex}-${index}`}
          onClick={() => onOpen(hit)}
        >
          <div className="search-hit-heading">
            <strong>{hit.documentName}</strong>
            <span>עמוד {hit.pageIndex + 1}</span>
          </div>
          <p>{hit.snippet}</p>
          <footer>
            <span>{hit.repositoryName}</span>
            <span>{hit.textSource}</span>
          </footer>
        </button>
      ))}
    </div>
  );
}
