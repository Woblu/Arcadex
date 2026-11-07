import type { LibraryStats, PlatformInfo } from "../types";
import { formatPlaytime } from "../api";
import SystemIcon from "./SystemIcon";
import Logo from "./Logo";

interface Props {
  platforms: PlatformInfo[];
  stats: LibraryStats | null;
  selected: string | null;
  favoritesOnly: boolean;
  unscrapedOnly: boolean;
  onSelectPlatform: (slug: string | null) => void;
  onToggleFavorites: () => void;
  onToggleUnscraped: () => void;
  onOpenSettings: () => void;
  onOpenStats: () => void;
}

export default function Sidebar({
  platforms,
  stats,
  selected,
  favoritesOnly,
  unscrapedOnly,
  onSelectPlatform,
  onToggleFavorites,
  onToggleUnscraped,
  onOpenSettings,
  onOpenStats,
}: Props) {
  const allActive = selected === null && !favoritesOnly && !unscrapedOnly;

  return (
    <aside className="sidebar">
      <div className="brand">
        <Logo size={22} className="brand-mark" />
        Arcadex
      </div>

      <div className="sidebar-scroll">
        <div className="nav-section">
          <button
            className={`nav-item ${allActive ? "active" : ""}`}
            onClick={() => onSelectPlatform(null)}
          >
            <SystemIcon platform="all" size={15} />
            All games
            <span className="count">{stats?.totalGames ?? 0}</span>
          </button>

          <button
            className={`nav-item ${favoritesOnly ? "active" : ""}`}
            onClick={onToggleFavorites}
          >
            <SystemIcon platform="star" size={15} />
            Favorites
          </button>

          <button
            className={`nav-item ${unscrapedOnly ? "active" : ""}`}
            onClick={onToggleUnscraped}
          >
            <SystemIcon platform="missing" size={15} />
            Needs metadata
            {stats && (
              <span className="count">
                {Math.max(stats.totalGames - stats.scraped, 0)}
              </span>
            )}
          </button>
        </div>

        {platforms.length > 0 && (
          <div className="nav-section">
            <div className="nav-label">Systems</div>
            {platforms.map((p) => (
              <button
                key={p.slug}
                className={`nav-item ${
                  selected === p.slug && !favoritesOnly && !unscrapedOnly
                    ? "active"
                    : ""
                }`}
                onClick={() => onSelectPlatform(p.slug)}
                title={p.name}
              >
                <SystemIcon platform={p.slug} size={15} />
                <span
                  style={{
                    overflow: "hidden",
                    textOverflow: "ellipsis",
                    whiteSpace: "nowrap",
                  }}
                >
                  {p.name}
                </span>
                <span className="count">{p.gameCount}</span>
              </button>
            ))}
          </div>
        )}
      </div>

      {stats && (
        <div className="stat-row">
          <span>{stats.scraped} scraped</span>
          <span>
            {stats.totalPlaySeconds > 0
              ? `${formatPlaytime(stats.totalPlaySeconds)} played`
              : "Nothing played yet"}
          </span>
        </div>
      )}

      <div className="sidebar-footer">
        <button className="btn ghost" onClick={onOpenStats}>
          Your library
        </button>
        <button className="btn ghost" onClick={onOpenSettings}>
          Settings
        </button>
      </div>
    </aside>
  );
}
