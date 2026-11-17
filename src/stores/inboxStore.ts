import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { AdcInboxItem, AdcInboxSummary, AdcInboxMarkReadResult } from '../types';

interface InboxState {
  unreadCount: number;
  messages: AdcInboxItem[];
  lastPolledAt: number | null;
  isOpen: boolean;
  isLoading: boolean;
  isSyncing: boolean;
  error: string | null;

  setOpen: (open: boolean) => void;
  toggleOpen: () => void;
  fetchSummary: () => Promise<void>;
  syncInbox: () => Promise<void>;
  markAsRead: (messageId: number) => Promise<void>;
  initListener: () => Promise<UnlistenFn>;
}

export const useInboxStore = create<InboxState>((set, get) => ({
  unreadCount: 0,
  messages: [],
  lastPolledAt: null,
  isOpen: false,
  isLoading: false,
  isSyncing: false,
  error: null,

  setOpen: (open: boolean) => set({ isOpen: open }),
  toggleOpen: () => set((state) => ({ isOpen: !state.isOpen })),

  fetchSummary: async () => {
    try {
      set({ isLoading: true, error: null });
      const summary = await invoke<AdcInboxSummary>('get_adc_inbox_summary', {
        limit: 20,
        offset: 0,
      });
      set({
        unreadCount: summary.unread_count,
        messages: summary.messages,
        lastPolledAt: summary.last_polled_at,
        isLoading: false,
      });
    } catch (err) {
      set({ error: String(err), isLoading: false });
    }
  },

  syncInbox: async () => {
    try {
      set({ isSyncing: true, error: null });
      const summary = await invoke<AdcInboxSummary>('sync_adc_inbox');
      set({
        unreadCount: summary.unread_count,
        messages: summary.messages,
        lastPolledAt: summary.last_polled_at,
        isSyncing: false,
      });
    } catch (err) {
      set({ error: String(err), isSyncing: false });
    }
  },

  markAsRead: async (messageId: number) => {
    try {
      const res = await invoke<AdcInboxMarkReadResult>('mark_adc_inbox_read', {
        messageId,
      });
      if (res.success) {
        set((state) => ({
          unreadCount: res.unread_count,
          messages: state.messages.map((m) =>
            m.id === messageId ? { ...m, pm_unread: false } : m,
          ),
        }));
      }
    } catch (err) {
      set({ error: String(err) });
    }
  },

  initListener: async () => {
    const unlisten = await listen<AdcInboxSummary>('nebulae://adc-inbox-updated', (event) => {
      set({
        unreadCount: event.payload.unread_count,
        messages: event.payload.messages,
        lastPolledAt: event.payload.last_polled_at,
      });
    });
    // Trigger initial fetch
    get().fetchSummary().catch(() => {});
    return unlisten;
  },
}));
