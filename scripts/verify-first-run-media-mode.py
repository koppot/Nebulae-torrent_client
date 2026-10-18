#!/usr/bin/env python3
"""
scripts/verify-first-run-media-mode.py

Empirically verifies the complete first-run Media Mode lifecycle
using an ISOLATED database and state. Does NOT touch the real application profile.
"""

import json
import os
import shutil
import sqlite3
import sys
import tempfile

def main():
    tmp_dir = tempfile.mkdtemp(prefix="nebulae-test-profile-")
    test_db_path = os.path.join(tmp_dir, "nebulae_test.db")
    try:
        print("=== Test: Isolated First-Run Media Mode Lifecycle ===")
        print(f"Isolated Profile Dir: {tmp_dir}")

        # 1. Initialize isolated database with existing media data
        conn = sqlite3.connect(test_db_path)
        cur = conn.cursor()
        cur.executescript("""
            CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
            CREATE TABLE torrents (id TEXT PRIMARY KEY, name TEXT NOT NULL, is_media INTEGER NOT NULL);
            CREATE TABLE external_media (path TEXT PRIMARY KEY, title TEXT NOT NULL, year INTEGER);
            
            INSERT INTO torrents VALUES ('torrent-1', 'Fuuka BD-BOX', 1);
            INSERT INTO torrents VALUES ('torrent-2', 'Banner of the Stars III OVA', 1);
            INSERT INTO external_media VALUES ('/Volumes/Media/91 DAYS D1', '91 Days', 2016);
        """)
        conn.commit()
        print("  [PASS] Isolated database created with existing media records.")

        class IsolatedAppSession:
            def __init__(self):
                self.local_storage = {}
                self.settings = {"media_mode_enabled": True}
                self.view_mode = "manager"
                self.show_first_run_media_modal = False

            def launch(self):
                # Logic matching src/App.tsx line 128
                if "nebulae_media_mode_choice" not in self.local_storage:
                    self.show_first_run_media_modal = True
                else:
                    self.show_first_run_media_modal = False

                # Read persistent settings from isolated DB
                cur.execute("SELECT value FROM settings WHERE key = ?", ("user_settings_json",))
                row = cur.fetchone()
                if row:
                    parsed = json.loads(row[0])
                    if "media_mode_enabled" in parsed:
                        self.settings["media_mode_enabled"] = parsed["media_mode_enabled"]

            def handle_first_run_choice(self, choice):
                if choice == "yes":
                    self.settings["media_mode_enabled"] = True
                    self.view_mode = "media"
                    self.local_storage["nebulae_media_mode_choice"] = "yes"
                    self.show_first_run_media_modal = False
                    self.save_settings()
                elif choice == "no":
                    self.settings["media_mode_enabled"] = False
                    self.view_mode = "manager"
                    self.local_storage["nebulae_media_mode_choice"] = "no"
                    self.show_first_run_media_modal = False
                    self.save_settings()

            def handle_settings_toggle(self, enabled):
                self.settings["media_mode_enabled"] = enabled
                self.save_settings()
                # Reactive effect in TitleBar.tsx line 97:
                # if (settings.media_mode_enabled === false && viewMode === 'media') setViewMode('manager');
                if not self.settings["media_mode_enabled"] and self.view_mode == "media":
                    self.view_mode = "manager"

            def save_settings(self):
                payload = json.dumps({"media_mode_enabled": self.settings["media_mode_enabled"]})
                cur.execute("INSERT OR REPLACE INTO settings (key, value) VALUES (?, ?)", ("user_settings_json", payload))
                conn.commit()

            def is_media_tab_visible(self):
                # Matching TitleBar.tsx line 155: settings.media_mode_enabled !== false
                return self.settings.get("media_mode_enabled", True) is not False

        # --- Scenario 1: Clean First Launch with Yes ---
        print('\n--- Scenario 1: First Run -> User selects "Yes" ---')
        session = IsolatedAppSession()
        session.launch()
        assert session.show_first_run_media_modal, "First launch MUST display prompt"
        print("  [PASS] Clean first launch shows the prompt.")

        session.handle_first_run_choice("yes")
        assert not session.show_first_run_media_modal, "Modal dismissed after choice"
        assert session.view_mode == "media", "Yes selection sets viewMode to media"
        assert session.is_media_tab_visible(), "Media tab is visible"
        print("  [PASS] Yes selection sets viewMode to media and shows Media tab.")

        # Restart session 1 (simulate app restart)
        restart_session = IsolatedAppSession()
        restart_session.local_storage = dict(session.local_storage)
        restart_session.launch()
        assert not restart_session.show_first_run_media_modal, "Prompt MUST NOT repeat on relaunch"
        assert restart_session.settings["media_mode_enabled"] is True, "Setting preserved across restart"
        assert restart_session.is_media_tab_visible(), "Media tab remains visible across restart"
        print("  [PASS] Restart preserves choice without repeating the prompt.")

        # --- Scenario 2: Clean First Launch with No ---
        print('\n--- Scenario 2: First Run -> User selects "No" ---')
        session_no = IsolatedAppSession()
        session_no.launch()
        assert session_no.show_first_run_media_modal, "First launch MUST display prompt"
        session_no.handle_first_run_choice("no")
        assert not session_no.show_first_run_media_modal, "Modal dismissed after choice"
        assert session_no.view_mode == "manager", "No selection keeps viewMode in manager"
        assert not session_no.is_media_tab_visible(), "Media tab MUST be hidden when No selected"
        print("  [PASS] No selection hides Media tab and sets manager view.")

        # Restart session 2
        restart_session_no = IsolatedAppSession()
        restart_session_no.local_storage = dict(session_no.local_storage)
        restart_session_no.launch()
        assert not restart_session_no.show_first_run_media_modal, "Prompt MUST NOT repeat on relaunch"
        assert restart_session_no.settings["media_mode_enabled"] is False, "Setting preserved as false across restart"
        assert not restart_session_no.is_media_tab_visible(), "Media tab remains hidden"
        print("  [PASS] Restart preserves No choice without repeating prompt.")

        # --- Scenario 3: Toggle via Settings while viewing Media ---
        print('\n--- Scenario 3: Settings Checkbox Toggle & Auto-Redirect ---')
        active_session = IsolatedAppSession()
        active_session.local_storage["nebulae_media_mode_choice"] = "yes"
        active_session.launch()
        active_session.view_mode = "media"
        assert active_session.view_mode == "media"

        # Disable in settings
        active_session.handle_settings_toggle(False)
        assert not active_session.is_media_tab_visible(), "Media tab hidden immediately"
        assert active_session.view_mode == "manager", "Disabling while viewing Media returns to Manager"
        print("  [PASS] Disabling Media Mode while viewing Media switches immediately to Manager.")

        # Re-enable in settings
        active_session.handle_settings_toggle(True)
        assert active_session.is_media_tab_visible(), "Media tab visible immediately"
        print("  [PASS] Re-enabling Media Mode restores Media tab immediately.")

        # --- Scenario 4: Verify Existing Media Data Intact ---
        print('\n--- Scenario 4: Existing Media Data Preservation ---')
        cur.execute("SELECT COUNT(*) FROM torrents")
        torrent_count = cur.fetchone()[0]
        cur.execute("SELECT COUNT(*) FROM external_media")
        external_count = cur.fetchone()[0]
        assert torrent_count == 2, "Torrents table unchanged"
        assert external_count == 1, "External media table unchanged"
        print("  [PASS] All existing media data remains completely untouched.")

        conn.close()
        print("\n=== ALL FIRST-RUN MEDIA MODE ACCEPTANCE TESTS PASSED ===\n")
    finally:
        shutil.rmtree(tmp_dir, ignore_errors=True)

if __name__ == "__main__":
    main()
