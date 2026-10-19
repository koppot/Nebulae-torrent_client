import React from 'react';
import ReactDOM from 'react-dom/client';
import App from './App';
import './index.css';

// Webview breadcrumbs: any uncaught error or rejected promise is written to
// the session log (via log_frontend) so "the button does nothing" reports
// come back with the exact JS failure instead of silence.
import { invoke } from '@tauri-apps/api/core';

const breadcrumb = (kind: string, detail: string) => {
  const clipped = detail.length > 500 ? `${detail.slice(0, 500)}…` : detail;
  invoke('log_frontend', { message: `${kind}: ${clipped}` }).catch(() => {});
};

window.addEventListener('error', (e) => {
  breadcrumb('js-error', `${e.message} @ ${e.filename}:${e.lineno}:${e.colno}`);
});
window.addEventListener('unhandledrejection', (e) => {
  const reason = e.reason instanceof Error ? e.reason.stack || e.reason.message : String(e.reason);
  breadcrumb('js-unhandled-rejection', reason);
});

ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
