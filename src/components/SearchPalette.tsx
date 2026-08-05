import { useEffect, useRef, useState } from "react";
import { searchNodes } from "../api/search";
import type { SearchHitDto } from "../types/node";

interface SearchPaletteProps {
  open: boolean;
  onClose: () => void;
  onSelect: (id: string) => void;
}

export function SearchPalette({ open, onClose, onSelect }: SearchPaletteProps) {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<SearchHitDto[]>([]);
  const inputRef = useRef<HTMLInputElement | null>(null);

  useEffect(() => {
    if (open) {
      setQuery("");
      setResults([]);
      setTimeout(() => inputRef.current?.focus(), 0);
    }
  }, [open]);

  useEffect(() => {
    if (!open || query.trim().length === 0) {
      setResults([]);
      return;
    }
    let cancelled = false;
    const timer = setTimeout(() => {
      searchNodes(query, { limit: 20 }).then((res) => {
        if (!cancelled) setResults(res.items);
      });
    }, 150);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [query, open]);

  if (!open) return null;

  return (
    <div className="dialog-backdrop" onClick={onClose}>
      <div className="dialog search-palette" onClick={(e) => e.stopPropagation()}>
        <input
          ref={inputRef}
          placeholder="Search notes…"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Escape") onClose();
            if (e.key === "Enter" && results[0]) {
              onSelect(results[0].id);
              onClose();
            }
          }}
        />
        <ul className="search-palette-results">
          {results.map((hit) => (
            <li key={hit.id}>
              <button
                className="search-palette-result"
                onClick={() => {
                  onSelect(hit.id);
                  onClose();
                }}
              >
                <span className="search-palette-result-title">{hit.title}</span>
                <span className="search-palette-result-snippet">{hit.snippet}</span>
              </button>
            </li>
          ))}
          {query.trim().length > 0 && results.length === 0 && (
            <li className="panel-empty">No matches for "{query}"</li>
          )}
        </ul>
      </div>
    </div>
  );
}
