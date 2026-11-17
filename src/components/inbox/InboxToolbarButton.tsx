import React, { useEffect } from 'react';
import { Mail } from 'lucide-react';
import { useInboxStore } from '../../stores/inboxStore';
import { InboxPopover } from './InboxPopover';

export const InboxToolbarButton: React.FC = () => {
  const { unreadCount, isOpen, toggleOpen, setOpen, initListener } = useInboxStore();

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    initListener().then((fn) => {
      unlisten = fn;
    });
    return () => {
      if (unlisten) unlisten();
    };
  }, [initListener]);

  const label = unreadCount > 0
    ? `ADC Inbox: ${unreadCount} unread message${unreadCount === 1 ? '' : 's'}`
    : 'ADC Inbox';

  return (
    <div className="relative">
      <button
        type="button"
        onClick={(e) => {
          e.stopPropagation();
          toggleOpen();
        }}
        className={`p-1.5 rounded transition relative ${
          isOpen
            ? 'bg-[#262626] text-white'
            : unreadCount > 0
            ? 'text-white hover:bg-[#1f1f1f]'
            : 'text-[#a3a3a3] hover:text-white hover:bg-[#1f1f1f]'
        }`}
        title={label}
        aria-label={label}
        aria-haspopup="dialog"
        aria-expanded={isOpen}
      >
        <Mail className="w-3.5 h-3.5" />
        {unreadCount > 0 && (
          <span className="absolute -top-1 -right-1 px-1 min-w-[15px] h-[15px] flex items-center justify-center rounded-full bg-[#e50914] text-[9px] font-bold text-white shadow-sm motion-safe:animate-pulse">
            {unreadCount > 99 ? '99+' : unreadCount}
          </span>
        )}
      </button>

      {isOpen && <InboxPopover onClose={() => setOpen(false)} />}
    </div>
  );
};

export default InboxToolbarButton;
