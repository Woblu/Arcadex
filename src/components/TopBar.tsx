import type { SortKey } from "../types";

interface Props {
  search: string;
  onSearch: (value: string) => void;
  sort: SortKey;
  onSort: (value: SortKey) => void;
  busy: boolean;
  count: number;
  onScan: () => void;
  onScrape: () => void;
}

const SORTS: Array<{ value: SortKey; label: string }> = [
  { value: "title", label: "Title" },
  { value: "recent", label: "Recently played" },
  { value: "played", label: "Most played" },
  { value: "added", label: "Recently added" },
  { value: "rating", label: "Rating" },
];

export default function TopBar({
  search,
  onSearch,
  sort,
  onSort,
  busy,
  count,
  onScan,
  onScrape,
}: Props) {
  return (
    <header className="topbar">
      <div className="search">
        <span className="icon">⌕</span>
        <input
          value={search}
          aria-label="Search your library"
          placeholder="Search your library…"
          onChange={(e) => onSearch(e.target.value)}
        />
      </div>

      <span className="card-sub">{count} shown</span>

      <div className="spacer" />

      <select
        className="select-inline"
        aria-label="Sort by"
        value={sort}
        onChange={(e) => onSort(e.target.value as SortKey)}
      >
        {SORTS.map((s) => (
          <option key={s.value} value={s.value}>
            {s.label}
          </option>
        ))}
      </select>

      <button className="btn" onClick={onScan} disabled={busy}>
        Scan folders
      </button>
      <button className="btn primary" onClick={onScrape} disabled={busy}>
        Fetch metadata
      </button>
    </header>
  );
}
