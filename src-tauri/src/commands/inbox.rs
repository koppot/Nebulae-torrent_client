use crate::db::{AdcInboxMessageRow, Database};
use crate::utils::keyring::{get_secret, passkey_secret_target};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::{Emitter, State};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdcInboxItem {
    pub id: u64,
    pub sender_id: u64,
    pub sender_username: String,
    pub subject: String,
    pub preview: String,
    pub added_at: String,
    pub pm_unread: bool,
}

impl From<AdcInboxMessageRow> for AdcInboxItem {
    fn from(row: AdcInboxMessageRow) -> Self {
        Self {
            id: row.id,
            sender_id: row.sender_id,
            sender_username: row.sender_username,
            subject: row.subject,
            preview: row.preview,
            added_at: row.added_at,
            pm_unread: row.pm_unread,
        }
    }
}

impl From<AdcInboxItem> for AdcInboxMessageRow {
    fn from(item: AdcInboxItem) -> Self {
        Self {
            id: item.id,
            sender_id: item.sender_id,
            sender_username: item.sender_username,
            subject: item.subject,
            preview: item.preview,
            added_at: item.added_at,
            pm_unread: item.pm_unread,
            fetched_at_epoch_ms: Utc::now().timestamp_millis(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdcInboxSummary {
    pub unread_count: u32,
    pub messages: Vec<AdcInboxItem>,
    pub last_polled_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdcInboxMarkReadResult {
    pub success: bool,
    pub message_id: u64,
    pub unread_count: u32,
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ServerInboxResponse {
    pub unread_count: Option<u32>,
    pub messages: Option<Vec<ServerInboxMessage>>,
}

#[derive(Debug, Deserialize)]
struct ServerInboxMessage {
    pub id: u64,
    pub sender_id: Option<u64>,
    pub sender_username: Option<String>,
    pub subject: Option<String>,
    pub preview: Option<String>,
    pub added: Option<String>,
    pub pm_unread: Option<i32>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct ServerMarkReadResponse {
    pub success: Option<bool>,
    pub unread_count: Option<u32>,
}

pub fn show_os_notification(title: &str, body: &str) {
    #[cfg(target_os = "macos")]
    {
        let script = format!(
            "display notification \"{}\" with title \"{}\"",
            body.replace('\\', "\\\\").replace('"', "\\\""),
            title.replace('\\', "\\\\").replace('"', "\\\"")
        );
        let _ = std::process::Command::new("osascript")
            .arg("-e")
            .arg(script)
            .spawn();
    }

    #[cfg(target_os = "windows")]
    {
        let escaped_title = title.replace('\'', "''");
        let escaped_body = body.replace('\'', "''");
        let script = format!(
            "[Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime] > $null; \
             $template = [Windows.UI.Notifications.ToastNotificationManager]::GetTemplateContent([Windows.UI.Notifications.ToastTemplateType]::ToastText02); \
             $textNodes = $template.GetElementsByTagName('text'); \
             $textNodes.Item(0).AppendChild($template.CreateTextNode('{escaped_title}')) > $null; \
             $textNodes.Item(1).AppendChild($template.CreateTextNode('{escaped_body}')) > $null; \
             $toast = [Windows.UI.Notifications.ToastNotification]::new($template); \
             [Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier('Nebulae').Show($toast);"
        );
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        let _ = std::process::Command::new("powershell")
            .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &script])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn();
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let _ = std::process::Command::new("notify-send")
            .arg(title)
            .arg(body)
            .spawn();
    }
}

pub fn fetch_and_persist_inbox(
    app: Option<&tauri::AppHandle>,
    db: &Database,
) -> Result<bool, String> {
    let is_linked = db
        .get_setting("adc_linked")
        .unwrap_or(None)
        .map(|v| v == "true")
        .unwrap_or(false);

    if !is_linked {
        return Ok(false);
    }

    let passkey = match get_secret(&passkey_secret_target()) {
        Ok(pk) => pk,
        Err(error) => {
            tracing::warn!(
                status = error.adc_state(),
                "[inbox-sync] ADC credential unavailable; skipping inbox fetch"
            );
            return Ok(false);
        }
    };

    let base_url = "https://asiandvdclub.org";
    let url = format!("{}/api/nebulae/inbox/summary", base_url);

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent("Nebulae/0.2.7")
        .build()
        .map_err(|e| format!("HTTP client build: {e}"))?;

    let payload = serde_json::json!({
        "passkey": passkey.trim(),
    });

    let resp = client
        .post(&url)
        .json(&payload)
        .send()
        .map_err(|e| format!("[inbox-sync] request failed: {e}"))?;

    let status = resp.status();

    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        tracing::info!("[inbox-sync] rate limited — skipping cycle");
        return Ok(false);
    }

    if status == reqwest::StatusCode::NOT_FOUND {
        tracing::debug!("[inbox-sync] inbox endpoint not yet enabled on server");
        return Ok(false);
    }

    if status == reqwest::StatusCode::FORBIDDEN || status == reqwest::StatusCode::UNAUTHORIZED {
        tracing::warn!("[inbox-sync] passkey rejected ({status})");
        return Err(format!("ADC returned {status}"));
    }

    if !status.is_success() {
        return Err(format!("ADC inbox returned unexpected status {status}"));
    }

    let parsed: ServerInboxResponse = resp
        .json()
        .map_err(|e| format!("[inbox-sync] parse response failed: {e}"))?;

    let total_unread = parsed.unread_count.unwrap_or(0);
    let mut rows: Vec<AdcInboxMessageRow> = Vec::new();

    if let Some(msgs) = parsed.messages {
        for m in msgs {
            rows.push(AdcInboxMessageRow {
                id: m.id,
                sender_id: m.sender_id.unwrap_or(0),
                sender_username: m
                    .sender_username
                    .unwrap_or_else(|| "ADC System".to_string()),
                subject: m.subject.unwrap_or_else(|| "(No Subject)".to_string()),
                preview: m.preview.unwrap_or_default(),
                added_at: m.added.unwrap_or_default(),
                pm_unread: m.pm_unread.unwrap_or(1) != 0,
                fetched_at_epoch_ms: Utc::now().timestamp_millis(),
            });
        }
    }

    // Persist to local database
    db.save_inbox_messages(&rows, total_unread)
        .map_err(|e| format!("DB save inbox: {e}"))?;

    // Notifications check
    let settings = crate::commands::settings::load_user_settings_from_db(db);
    let last_notified_id = db.get_inbox_last_notified_id().unwrap_or(0);

    let new_unread: Vec<&AdcInboxMessageRow> = rows
        .iter()
        .filter(|m| m.pm_unread && m.id > last_notified_id)
        .collect();

    if !new_unread.is_empty() {
        let max_id = new_unread
            .iter()
            .map(|m| m.id)
            .max()
            .unwrap_or(last_notified_id);
        let _ = db.set_inbox_last_notified_id(max_id);

        if settings.notify_on_inbox_message {
            if settings.notify_inbox_previews {
                if let Some(newest) = new_unread.first() {
                    let title = format!("New message from {}", newest.sender_username);
                    let body = if newest.preview.trim().is_empty() {
                        newest.subject.clone()
                    } else {
                        format!("{}: {}", newest.subject, newest.preview)
                    };
                    show_os_notification(&title, &body);
                }
            } else {
                let title = "Asian DVD Club Inbox".to_string();
                let body = format!(
                    "You have {} unread message{}",
                    total_unread,
                    if total_unread == 1 { "" } else { "s" }
                );
                show_os_notification(&title, &body);
            }
        }
    }

    if let Some(app_handle) = app {
        let summary = build_inbox_summary(db, 20, 0)?;
        let _ = app_handle.emit("nebulae://adc-inbox-updated", &summary);
    }

    Ok(true)
}

fn build_inbox_summary(db: &Database, limit: u32, offset: u32) -> Result<AdcInboxSummary, String> {
    let (rows, unread_count) = db
        .get_inbox_messages(limit, offset)
        .map_err(|e| format!("DB get inbox: {e}"))?;

    let last_polled_at = db
        .get_setting("adc_inbox_last_polled_at")
        .unwrap_or(None)
        .and_then(|s| s.parse::<i64>().ok());

    let messages = rows.into_iter().map(AdcInboxItem::from).collect();

    Ok(AdcInboxSummary {
        unread_count,
        messages,
        last_polled_at,
    })
}

#[tauri::command]
pub async fn get_adc_inbox_summary(
    db: State<'_, Arc<Database>>,
    limit: Option<u32>,
    offset: Option<u32>,
) -> Result<AdcInboxSummary, String> {
    let db = Arc::clone(&db);
    let lim = limit.unwrap_or(20);
    let off = offset.unwrap_or(0);
    tokio::task::spawn_blocking(move || build_inbox_summary(&db, lim, off))
        .await
        .map_err(|e| format!("spawn_blocking: {e}"))?
}

#[tauri::command]
pub async fn sync_adc_inbox(
    app: tauri::AppHandle,
    db: State<'_, Arc<Database>>,
) -> Result<AdcInboxSummary, String> {
    let db_arc = Arc::clone(&db);
    let app_clone = app.clone();
    tokio::task::spawn_blocking(move || {
        let _ = fetch_and_persist_inbox(Some(&app_clone), &db_arc);
    })
    .await
    .map_err(|e| format!("spawn_blocking: {e}"))?;

    get_adc_inbox_summary(db, Some(20), Some(0)).await
}

#[tauri::command]
pub async fn mark_adc_inbox_read(
    app: tauri::AppHandle,
    db: State<'_, Arc<Database>>,
    message_id: u64,
) -> Result<AdcInboxMarkReadResult, String> {
    let db_arc = Arc::clone(&db);
    let new_unread = tokio::task::spawn_blocking(move || {
        db_arc
            .mark_inbox_message_read(message_id)
            .map_err(|e| format!("mark message read: {e}"))
    })
    .await
    .map_err(|e| format!("spawn_blocking: {e}"))??;

    // Emit updated summary immediately to frontend
    if let Ok(summary) = build_inbox_summary(&db, 20, 0) {
        let _ = app.emit("nebulae://adc-inbox-updated", &summary);
    }

    // Attempt remote sync in background
    tauri::async_runtime::spawn(async move {
        let _ = tokio::task::spawn_blocking(move || {
            let passkey = match get_secret(&passkey_secret_target()) {
                Ok(pk) => pk,
                Err(_) => return,
            };
            let client = match reqwest::blocking::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .user_agent("Nebulae/0.2.7")
                .build()
            {
                Ok(c) => c,
                Err(_) => return,
            };
            let url = "https://asiandvdclub.org/api/nebulae/inbox/mark_read";
            let payload = serde_json::json!({
                "passkey": passkey.trim(),
                "message_id": message_id,
            });
            let _ = client.post(url).json(&payload).send();
        })
        .await;
    });

    Ok(AdcInboxMarkReadResult {
        success: true,
        message_id,
        unread_count: new_unread,
        error: None,
    })
}

#[tauri::command]
pub async fn clear_adc_inbox(
    app: tauri::AppHandle,
    db: State<'_, Arc<Database>>,
) -> Result<(), String> {
    let db_arc = Arc::clone(&db);
    tokio::task::spawn_blocking(move || {
        db_arc
            .clear_inbox_cache()
            .map_err(|e| format!("clear inbox cache: {e}"))
    })
    .await
    .map_err(|e| format!("spawn_blocking: {e}"))??;

    if let Ok(summary) = build_inbox_summary(&db, 20, 0) {
        let _ = app.emit("nebulae://adc-inbox-updated", &summary);
    }
    Ok(())
}

pub fn spawn_inbox_poller(app: tauri::AppHandle, db: Arc<Database>) {
    tauri::async_runtime::spawn(async move {
        // Initial delay to avoid startup contention
        tokio::time::sleep(std::time::Duration::from_secs(15)).await;

        loop {
            let db_ref = Arc::clone(&db);
            let app_handle = app.clone();
            let result = tokio::task::spawn_blocking(move || {
                fetch_and_persist_inbox(Some(&app_handle), &db_ref)
            })
            .await;

            match result {
                Ok(Ok(true)) => tracing::info!("[inbox-poller] inbox refresh OK"),
                Ok(Ok(false)) => {} // Silent
                Ok(Err(e)) => tracing::warn!("[inbox-poller] fetch failed: {e}"),
                Err(e) => tracing::error!("[inbox-poller] thread join error: {e}"),
            }

            // Sleep 5 minutes between polls
            tokio::time::sleep(std::time::Duration::from_secs(300)).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_inbox_db_lifecycle_and_unread_count() {
        let db = Database::new(Path::new(":memory:")).expect("create test db");

        // Initially empty
        let (msgs, unread) = db.get_inbox_messages(10, 0).expect("get inbox");
        assert_eq!(msgs.len(), 0);
        assert_eq!(unread, 0);

        // Populate two test messages
        let items = vec![
            AdcInboxMessageRow {
                id: 101,
                sender_id: 1,
                sender_username: "Admin".to_string(),
                subject: "Welcome to ADC".to_string(),
                preview: "Please read the rules".to_string(),
                added_at: "2026-11-17 12:00:00".to_string(),
                pm_unread: true,
                fetched_at_epoch_ms: 1000,
            },
            AdcInboxMessageRow {
                id: 102,
                sender_id: 2,
                sender_username: "Moderator".to_string(),
                subject: "Reseed request".to_string(),
                preview: "Could you reseed torrent 45?".to_string(),
                added_at: "2026-11-17 12:30:00".to_string(),
                pm_unread: true,
                fetched_at_epoch_ms: 2000,
            },
        ];

        db.save_inbox_messages(&items, 2).expect("save messages");

        let (msgs, unread) = db.get_inbox_messages(10, 0).expect("get inbox");
        assert_eq!(msgs.len(), 2);
        assert_eq!(unread, 2);
        // Ordered by id DESC
        assert_eq!(msgs[0].id, 102);
        assert_eq!(msgs[1].id, 101);

        // Mark message 102 as read
        let new_unread = db.mark_inbox_message_read(102).expect("mark read");
        assert_eq!(new_unread, 1);

        let (msgs, unread) = db.get_inbox_messages(10, 0).expect("get inbox");
        assert_eq!(unread, 1);
        assert!(!msgs[0].pm_unread);
        assert!(msgs[1].pm_unread);

        // Pagination check
        let (paged, _) = db.get_inbox_messages(1, 1).expect("get paged");
        assert_eq!(paged.len(), 1);
        assert_eq!(paged[0].id, 101);

        // Test last notified ID tracking
        assert_eq!(db.get_inbox_last_notified_id().unwrap(), 0);
        db.set_inbox_last_notified_id(102).unwrap();
        assert_eq!(db.get_inbox_last_notified_id().unwrap(), 102);

        // Clear inbox cache on account switch/unlink
        db.clear_inbox_cache().expect("clear inbox");
        let (msgs_cleared, unread_cleared) = db.get_inbox_messages(10, 0).expect("get inbox");
        assert_eq!(msgs_cleared.len(), 0);
        assert_eq!(unread_cleared, 0);
        assert_eq!(db.get_inbox_last_notified_id().unwrap(), 0);
    }
}
