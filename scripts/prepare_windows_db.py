import sqlite3
import json

db_path = r"C:\Users\administrator\AppData\Roaming\Nebulae\internal\db\nebulae.db"
conn = sqlite3.connect(db_path)
cur = conn.cursor()

cur.execute("SELECT value FROM settings WHERE key = 'user_settings_json'")
row = cur.fetchone()
if row:
    s = json.loads(row[0])
else:
    s = {}

s["close_to_tray"] = False
s["auto_update_enabled"] = True

cur.execute("INSERT OR REPLACE INTO settings (key, value) VALUES ('user_settings_json', ?)", (json.dumps(s),))

# Seed test torrents into torrent_metadata
cur.execute("INSERT OR IGNORE INTO torrent_metadata (info_hash, display_title, added_at) VALUES ('hash-win-111', 'Fuuka BD-BOX', 1729200000)")
cur.execute("INSERT OR IGNORE INTO torrent_metadata (info_hash, display_title, added_at) VALUES ('hash-win-222', 'Banner of the Stars III OVA', 1729201000)")

conn.commit()
conn.close()
print("SUCCESS: Configured close_to_tray=False, auto_update_enabled=True, and seeded test torrents in internal\\db\\nebulae.db.")
