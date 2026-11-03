/**
 * MetadataEditModal — F8/F9
 *
 * Opened via the "Edit" button in the media details popup.
 * Lets the user search TMDb, IMDb (via TMDb), or MAL and select a
 * correct title, then confirms replacing the existing metadata entry.
 *
 * Behaviour reference: ADC upload.php title-search-and-select workflow.
 */
import React, { useState, useEffect, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { X, Search, Loader2, CheckCircle2, AlertCircle } from 'lucide-react';

// ── Types ────────────────────────────────────────────────────────────────────

type Provider = 'tmdb' | 'mal';

interface TmdbSearchResult {
  tmdb_id: number;
  title: string;
  year: number | null;
  overview: string | null;
  poster_url: string | null; // raw TMDB URL (w500)
  media_type: string; // 'movie' | 'tv'
  /** IMDb ID (e.g. "tt1234567") when available from TMDb's external_ids. */
  imdb_id: string | null;
}

interface MalSearchResult {
  mal_id: number;
  title: string;
  year: number | null;
  synopsis: string | null;
  poster_url: string | null;
}

type SearchResult =
  | { provider: 'tmdb'; data: TmdbSearchResult }
  | { provider: 'mal'; data: MalSearchResult };

// ── Helpers ───────────────────────────────────────────────────────────────────

function resultId(r: SearchResult): string {
  if (r.provider === 'tmdb') return `tmdb-${r.data.tmdb_id}`;
  return `mal-${r.data.mal_id}`;
}

function resultTitle(r: SearchResult): string {
  return r.data.title;
}

function resultYear(r: SearchResult): number | null {
  return r.data.year;
}

function resultOverview(r: SearchResult): string | null {
  if (r.provider === 'tmdb') return r.data.overview;
  return r.data.synopsis;
}

function resultPoster(r: SearchResult): string | null {
  if (r.provider === 'tmdb') return r.data.poster_url;
  return r.data.poster_url;
}

// ── Component ─────────────────────────────────────────────────────────────────

interface Props {
  infoHash: string;
  currentTitle: string;
  onClose: () => void;
  onConfirmed: (newTitle: string, provider: string, providerId: string, posterUrl?: string) => void;
}

export const MetadataEditModal: React.FC<Props> = ({
  infoHash,
  currentTitle,
  onClose,
  onConfirmed,
}) => {
  const [query, setQuery] = useState(currentTitle);
  const [provider, setProvider] = useState<Provider>('tmdb');
  const [results, setResults] = useState<SearchResult[]>([]);
  const [selected, setSelected] = useState<SearchResult | null>(null);
  const [isSearching, setIsSearching] = useState(false);
  const [isConfirming, setIsConfirming] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  // Auto-focus the search field on mount.
  useEffect(() => {
    inputRef.current?.select();
  }, []);

  const runSearch = async () => {
    const q = query.trim();
    if (!q) return;
    setIsSearching(true);
    setError(null);
    setResults([]);
    setSelected(null);
    try {
      if (provider === 'tmdb') {
        const raw = await invoke<TmdbSearchResult[]>('search_tmdb_titles', { query: q });
        setResults((raw ?? []).map((d) => ({ provider: 'tmdb' as const, data: d })));
      } else {
        const raw = await invoke<MalSearchResult[]>('search_mal_titles', { query: q });
        setResults((raw ?? []).map((d) => ({ provider: 'mal' as const, data: d })));
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setIsSearching(false);
    }
  };

  const handleConfirm = async () => {
    if (!selected) return;
    setIsConfirming(true);
    setError(null);
    try {
      const pid =
        selected.provider === 'tmdb'
          ? String(selected.data.tmdb_id)
          : String(selected.data.mal_id);

      const newTitle = resultTitle(selected);
      const posterUrl = resultPoster(selected);

      // Save user title, provider attribution, and poster directly to database
      await invoke('save_media_metadata_and_poster', {
        idOrHashOrPath: infoHash,
        title: newTitle,
        provider: selected.provider,
        providerId: pid,
        posterUrl: posterUrl ?? null,
      });

      // Best-effort provider cache fetch
      try {
        if (selected.provider === 'tmdb') {
          const year = resultYear(selected);
          await invoke('fetch_tmdb_release', { title: newTitle, year });
        } else {
          await invoke('fetch_mal_anime', { title: newTitle });
        }
      } catch (posterErr) {
        console.warn('[MetadataEditModal] poster fetch failed:', posterErr);
      }

      onConfirmed(newTitle, selected.provider, pid, posterUrl ?? undefined);
    } catch (e) {
      setError(String(e));
    } finally {
      setIsConfirming(false);
    }
  };

  return (
    <div
      className="fixed inset-0 z-[300] flex items-center justify-center bg-black/80 backdrop-blur-sm p-4"
      onClick={onClose}
    >
      <div
        className="w-full max-w-lg max-h-[85vh] flex flex-col bg-[#141414] border border-[#333333] rounded-xl shadow-2xl text-white"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div className="flex items-center justify-between px-5 py-3.5 border-b border-[#333333] bg-[#1f1f1f] shrink-0">
          <h2 className="text-sm font-bold">Edit Metadata</h2>
          <button
            onClick={onClose}
            className="p-1 text-[#a3a3a3] hover:text-white transition"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        {/* Body */}
        <div className="flex-1 overflow-y-auto p-5 space-y-4 text-xs">
          {/* Provider tabs */}
          <div className="flex gap-1 bg-[#0a0a0a] rounded-lg p-1">
            {(['tmdb', 'mal'] as Provider[]).map((p) => (
              <button
                key={p}
                onClick={() => {
                  setProvider(p);
                  setResults([]);
                  setSelected(null);
                  setError(null);
                }}
                className={`flex-1 py-1.5 rounded-md text-xs font-medium transition ${
                  provider === p
                    ? 'bg-[#e50914] text-white'
                    : 'text-[#a3a3a3] hover:text-white'
                }`}
              >
                {p === 'tmdb' ? 'TMDb / IMDb' : 'MyAnimeList'}
              </button>
            ))}
          </div>

          {/* Search field */}
          <div className="flex gap-2">
            <input
              ref={inputRef}
              type="text"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              onKeyDown={(e) => e.key === 'Enter' && runSearch()}
              placeholder="Title to search…"
              className="flex-1 px-3 py-1.5 rounded border border-[#333333] bg-[#0a0a0a] text-white text-xs focus:border-[#e50914] focus:outline-none"
            />
            <button
              onClick={runSearch}
              disabled={isSearching || !query.trim()}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded bg-[#e50914] hover:bg-[#f40612] text-white text-xs font-semibold transition disabled:opacity-40"
            >
              {isSearching ? (
                <Loader2 className="w-3.5 h-3.5 animate-spin" />
              ) : (
                <Search className="w-3.5 h-3.5" />
              )}
              {isSearching ? 'Searching…' : 'Search'}
            </button>
          </div>

          {/* Error */}
          {error && (
            <div className="flex items-center gap-1.5 text-[#f87171] text-xs">
              <AlertCircle className="w-3.5 h-3.5 shrink-0" />
              <span>{error}</span>
            </div>
          )}

          {/* Results list */}
          {results.length > 0 && (
            <div className="space-y-2">
              <span className="text-[10px] text-[#737373] uppercase tracking-wider font-mono">
                {results.length} result{results.length !== 1 ? 's' : ''} — select the correct title
              </span>
              <div className="space-y-1.5 max-h-64 overflow-y-auto pr-0.5">
                {results.map((r) => {
                  const rid = resultId(r);
                  const isSelected = selected && resultId(selected) === rid;
                  return (
                    <button
                      key={rid}
                      onClick={() => setSelected(r)}
                      className={`w-full flex items-start gap-3 p-2.5 rounded-lg border text-left transition ${
                        isSelected
                          ? 'border-[#e50914] bg-[#1a0a0a]'
                          : 'border-[#262626] bg-[#0f0f0f] hover:border-[#444444]'
                      }`}
                    >
                      {/* Poster thumbnail */}
                      <div className="w-10 shrink-0 aspect-[2/3] rounded overflow-hidden bg-[#1a1a1a]">
                        {resultPoster(r) ? (
                          <img
                            src={resultPoster(r)!}
                            alt=""
                            className="w-full h-full object-cover"
                            draggable={false}
                          />
                        ) : (
                          <div className="w-full h-full flex items-center justify-center text-[#555555] text-[8px] font-mono">
                            N/A
                          </div>
                        )}
                      </div>

                      {/* Info */}
                      <div className="flex-1 min-w-0 space-y-0.5">
                        <div className="flex items-center gap-1.5">
                          {isSelected && (
                            <CheckCircle2 className="w-3 h-3 text-[#e50914] shrink-0" />
                          )}
                          <span className="text-xs font-semibold text-white truncate">
                            {resultTitle(r)}
                          </span>
                          {resultYear(r) && (
                            <span className="text-[10px] text-[#737373] font-mono shrink-0">
                              {resultYear(r)}
                            </span>
                          )}
                        </div>
                        {resultOverview(r) && (
                          <p className="text-[10px] text-[#8c8c8c] line-clamp-2 leading-relaxed">
                            {resultOverview(r)}
                          </p>
                        )}
                        <span className="inline-block text-[9px] px-1.5 py-0.5 rounded bg-[#262626] text-[#737373] font-mono uppercase">
                          {r.provider === 'tmdb'
                            ? (r.data as TmdbSearchResult).media_type || 'tmdb'
                            : 'anime'}
                        </span>
                        {/* IMDb ID badge — shown when TMDb provides the mapping */}
                        {r.provider === 'tmdb' && (r.data as TmdbSearchResult).imdb_id && (
                          <a
                            href={`https://www.imdb.com/title/${(r.data as TmdbSearchResult).imdb_id}/`}
                            target="_blank"
                            rel="noopener noreferrer"
                            onClick={(e) => e.stopPropagation()}
                            className="inline-block text-[9px] px-1.5 py-0.5 rounded bg-[#1a1400] border border-[#f5c518]/30 text-[#f5c518] font-mono hover:border-[#f5c518]/60 transition"
                            title="Open on IMDb"
                          >
                            {(r.data as TmdbSearchResult).imdb_id}
                          </a>
                        )}
                        {/* MAL ID badge */}
                        {r.provider === 'mal' && (
                          <span className="inline-block text-[9px] px-1.5 py-0.5 rounded bg-[#262626] text-[#4a90d9] font-mono">
                            MAL #{(r.data as MalSearchResult).mal_id}
                          </span>
                        )}
                      </div>
                    </button>
                  );
                })}
              </div>
            </div>
          )}

          {results.length === 0 && !isSearching && !error && (
            <p className="text-[#737373] italic text-center py-4">
              Enter a title and press Search to find matching results.
            </p>
          )}
        </div>

        {/* Footer */}
        <div className="px-5 py-3 border-t border-[#333333] bg-[#1f1f1f] flex items-center justify-between shrink-0">
          <div className="text-[10px] text-[#737373]">
            {selected
              ? `Selected: ${resultTitle(selected)}${resultYear(selected) ? ` (${resultYear(selected)})` : ''}`
              : 'No result selected'}
          </div>
          <div className="flex gap-2">
            <button
              onClick={onClose}
              className="px-3 py-1.5 text-xs rounded border border-[#333333] bg-[#141414] hover:bg-[#262626] text-white transition"
            >
              Cancel
            </button>
            <button
              onClick={handleConfirm}
              disabled={!selected || isConfirming}
              className="px-4 py-1.5 text-xs rounded bg-[#e50914] hover:bg-[#f40612] text-white font-semibold transition disabled:opacity-40"
            >
              {isConfirming ? 'Applying…' : 'Replace Metadata'}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
};

export default MetadataEditModal;
