import React, { useEffect, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useInboxStore } from '../../stores/inboxStore';
import { Mail, RefreshCw, ExternalLink, Check, X, BellOff } from 'lucide-react';

interface InboxPopoverProps {
  onClose: () => void;
}

export const InboxPopover: React.FC<InboxPopoverProps> = ({ onClose }) => {
  const {
    unreadCount,
    messages,
    isLoading,
    isSyncing,
    error,
    syncInbox,
    markAsRead,
    fetchSummary,
  } = useInboxStore();

  const popoverRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    fetchSummary().catch(() => {});
  }, [fetchSummary]);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        onClose();
      }
    };
    const handleClickOutside = (e: MouseEvent) => {
      if (popoverRef.current && !popoverRef.current.contains(e.target as Node)) {
        onClose();
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    window.addEventListener('mousedown', handleClickOutside);
    return () => {
      window.removeEventListener('keydown', handleKeyDown);
      window.removeEventListener('mousedown', handleClickOutside);
    };
  }, [onClose]);

  const handleOpenAdcInbox = (e: React.MouseEvent) => {
    e.stopPropagation();
    invoke('open_external_link', { url: 'https://asiandvdclub.org/messages/inbox' }).catch(() => {});
  };

  const handleOpenMessage = (id: number) => {
    invoke('open_external_link', { url: `https://asiandvdclub.org/message/${id}` }).catch(() => {});
  };

  return (
    <div
      ref={popoverRef}
      role="dialog"
      aria-label="ADC Private Messages Inbox"
      className="absolute right-0 top-full mt-2 w-96 max-w-[calc(100vw-24px)] bg-[#1a1a1a] border border-[#333333] rounded-lg shadow-2xl z-50 overflow-hidden text-xs flex flex-col font-sans"
      onClick={(e) => e.stopPropagation()}
    >
      {/* Header */}
      <div className="flex items-center justify-between px-3 py-2.5 border-b border-[#2d2d2d] bg-[#141414]">
        <div className="flex items-center gap-2">
          <Mail className="w-4 h-4 text-[#e50914]" />
          <span className="font-semibold text-white tracking-tight">ADC Inbox</span>
          {unreadCount > 0 && (
            <span className="px-1.5 py-0.2 bg-[#e50914] text-white text-[10px] font-bold rounded-full">
              {unreadCount} unread
            </span>
          )}
        </div>
        <div className="flex items-center gap-1">
          <button
            type="button"
            onClick={() => syncInbox()}
            disabled={isSyncing}
            className="p-1 rounded hover:bg-[#262626] text-[#a3a3a3] hover:text-white transition disabled:opacity-40"
            title="Refresh Inbox"
            aria-label="Refresh Inbox"
          >
            <RefreshCw className={`w-3.5 h-3.5 ${isSyncing ? 'motion-safe:animate-spin text-[#e50914]' : ''}`} />
          </button>
          <button
            type="button"
            onClick={handleOpenAdcInbox}
            className="p-1 rounded hover:bg-[#262626] text-[#a3a3a3] hover:text-white transition"
            title="Open on AsianDVDClub.org"
            aria-label="Open on AsianDVDClub.org"
          >
            <ExternalLink className="w-3.5 h-3.5" />
          </button>
          <button
            type="button"
            onClick={onClose}
            className="p-1 rounded hover:bg-[#262626] text-[#a3a3a3] hover:text-white transition"
            title="Close"
            aria-label="Close"
          >
            <X className="w-3.5 h-3.5" />
          </button>
        </div>
      </div>

      {/* Error Banner if any */}
      {error && (
        <div className="px-3 py-1.5 bg-red-950/40 border-b border-red-900/50 text-red-200 text-[11px]">
          {error}
        </div>
      )}

      {/* Messages List */}
      <div className="max-h-80 overflow-y-auto divide-y divide-[#262626]">
        {isLoading && messages.length === 0 ? (
          <div className="py-8 text-center text-[#737373] text-xs">
            Loading messages…
          </div>
        ) : messages.length === 0 ? (
          <div className="py-8 flex flex-col items-center justify-center gap-2 text-[#737373]">
            <BellOff className="w-7 h-7 opacity-50" />
            <span>No messages in inbox</span>
          </div>
        ) : (
          messages.map((item) => (
            <div
              key={item.id}
              onClick={() => handleOpenMessage(item.id)}
              className={`p-3 cursor-pointer transition hover:bg-[#222222] flex gap-2.5 items-start relative ${
                item.pm_unread ? 'bg-[#1f1f1f]/80' : 'bg-transparent opacity-85'
              }`}
            >
              {/* Unread indicator dot */}
              <div className="mt-1 flex-shrink-0">
                {item.pm_unread ? (
                  <span className="block w-2 h-2 rounded-full bg-[#e50914]" />
                ) : (
                  <span className="block w-2 h-2 rounded-full bg-transparent" />
                )}
              </div>

              {/* Message Details */}
              <div className="flex-1 min-w-0">
                <div className="flex items-center justify-between gap-1 mb-0.5">
                  <span className={`truncate text-white font-medium ${item.pm_unread ? 'font-bold' : ''}`}>
                    {item.sender_username}
                  </span>
                  <span className="text-[10px] text-[#737373] flex-shrink-0">
                    {item.added_at}
                  </span>
                </div>
                <div className={`text-xs truncate mb-1 ${item.pm_unread ? 'text-white font-semibold' : 'text-[#d4d4d4]'}`}>
                  {item.subject}
                </div>
                {item.preview && (
                  <p className="text-[11px] text-[#8e8e8e] line-clamp-2 leading-relaxed">
                    {item.preview}
                  </p>
                )}
              </div>

              {/* Mark as read button (individual per-message sync) */}
              {item.pm_unread && (
                <button
                  type="button"
                  onClick={(e) => {
                    e.stopPropagation();
                    markAsRead(item.id);
                  }}
                  className="p-1 rounded hover:bg-[#333333] text-[#a3a3a3] hover:text-emerald-400 transition flex-shrink-0"
                  title="Mark as read"
                  aria-label="Mark message as read"
                >
                  <Check className="w-3.5 h-3.5" />
                </button>
              )}
            </div>
          ))
        )}
      </div>

      {/* Footer */}
      <div className="px-3 py-2 bg-[#141414] border-t border-[#262626] flex items-center justify-between text-[11px] text-[#737373]">
        <span>
          {unreadCount > messages.length
            ? `Showing recent ${messages.length} of ${unreadCount} unread`
            : "Click message to open on ADC"}
        </span>
        <button
          type="button"
          onClick={handleOpenAdcInbox}
          className="text-[#e50914] hover:underline font-medium"
        >
          View all on website →
        </button>
      </div>
    </div>
  );
};

export default InboxPopover;
