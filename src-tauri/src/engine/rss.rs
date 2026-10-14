use crate::commands::settings::UserSettings;
use crate::db::Database;
use crate::engine::SharedEngine;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tracing::{info, warn};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RssItem {
    pub title: String,
    pub link: String,
    pub torrent_url: Option<String>,
    pub pub_date: Option<String>,
}

pub fn parse_rss_xml(xml: &str) -> Vec<RssItem> {
    let mut items = Vec::new();
    let mut current_item_xml = String::new();
    let mut inside_item = false;

    for line in xml.lines() {
        let trimmed = line.trim();
        if trimmed.contains("<item>") || trimmed.contains("<item ") {
            inside_item = true;
            current_item_xml.clear();
        }

        if inside_item {
            current_item_xml.push_str(trimmed);
            current_item_xml.push(' ');
        }

        if trimmed.contains("</item>") && inside_item {
            inside_item = false;
            if let Some(parsed) = extract_rss_item(&current_item_xml) {
                items.push(parsed);
            }
            current_item_xml.clear();
        }
    }

    items
}

fn extract_rss_item(block: &str) -> Option<RssItem> {
    let extract_tag = |tag: &str| -> Option<String> {
        let open = format!("<{tag}>");
        let close = format!("</{tag}>");
        let start = block.find(&open)?;
        let end = block.find(&close)?;
        if end > start + open.len() {
            let content = &block[start + open.len()..end];
            // Handle CDATA if present
            let un_cdata = if content.starts_with("<![CDATA[") && content.ends_with("]]>") {
                &content[9..content.len() - 3]
            } else {
                content
            };
            Some(un_cdata.trim().to_string())
        } else {
            None
        }
    };

    let title = extract_tag("title")?;
    let link = extract_tag("link").unwrap_or_default();
    let pub_date = extract_tag("pubDate");

    // Check for enclosure url
    let torrent_url = if let Some(enc_pos) = block.find("<enclosure") {
        let slice = &block[enc_pos..];
        if let Some(url_pos) = slice.find("url=\"") {
            let url_slice = &slice[url_pos + 5..];
            url_slice
                .find('"')
                .map(|quote_end| url_slice[..quote_end].to_string())
        } else {
            None
        }
    } else if link.ends_with(".torrent") || link.contains("download.php") {
        Some(link.clone())
    } else {
        None
    };

    Some(RssItem {
        title,
        link,
        torrent_url,
        pub_date,
    })
}

pub fn spawn_rss_loop(engine: SharedEngine, db: Arc<Database>) {
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .unwrap_or_default();

        loop {
            interval.tick().await;

            let settings = if let Ok(Some(json)) = db.get_setting("user_settings_json") {
                serde_json::from_str::<UserSettings>(&json).unwrap_or_default()
            } else {
                UserSettings::default()
            };

            if !settings.rss_auto_downloader_enabled {
                continue;
            }

            // Read list of configured RSS feeds
            let feeds_json = db.get_setting("rss_feeds_json").unwrap_or(None);
            let feed_urls: Vec<String> = if let Some(ref j) = feeds_json {
                serde_json::from_str(j).unwrap_or_default()
            } else {
                Vec::new()
            };

            if feed_urls.is_empty() {
                continue;
            }

            let downloaded_json = db.get_setting("rss_downloaded_urls_json").unwrap_or(None);
            let mut downloaded_urls: std::collections::HashSet<String> =
                if let Some(ref dj) = downloaded_json {
                    serde_json::from_str(dj).unwrap_or_default()
                } else {
                    std::collections::HashSet::new()
                };

            for feed_url in feed_urls {
                let response = match client.get(&feed_url).send().await {
                    Ok(r) => r,
                    Err(e) => {
                        warn!("RSS feed fetch error for {feed_url}: {e}");
                        continue;
                    }
                };

                let body = match response.text().await {
                    Ok(t) => t,
                    Err(_) => continue,
                };

                let items = parse_rss_xml(&body);
                for item in items {
                    if let Some(ref t_url) = item.torrent_url {
                        if !downloaded_urls.contains(t_url) {
                            // Download .torrent bytes
                            if let Ok(torrent_resp) = client.get(t_url).send().await {
                                if let Ok(bytes) = torrent_resp.bytes().await {
                                    match engine.add_torrent_from_bytes(&bytes, None) {
                                        Ok(id) => {
                                            info!(
                                                "RSS Auto-downloader: added torrent {id} for '{}'",
                                                item.title
                                            );
                                            downloaded_urls.insert(t_url.clone());
                                        }
                                        Err(e) => {
                                            warn!(
                                                "RSS Auto-downloader: failed to add '{}': {e}",
                                                item.title
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if let Ok(ser) = serde_json::to_string(&downloaded_urls) {
                let _ = db.set_setting("rss_downloaded_urls_json", &ser);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_rss_xml() {
        let sample = r#"<?xml version="1.0" encoding="UTF-8"?>
        <rss version="2.0">
          <channel>
            <title>Asian DVD Club Torrents</title>
            <item>
              <title><![CDATA[Tokyo Story (1953) [4K Restoration]]]></title>
              <link>https://asiandvdclub.org/details.php?id=12345</link>
              <enclosure url="https://asiandvdclub.org/download.php?id=12345&amp;passkey=testpasskey" length="34500000" type="application/x-bittorrent" />
              <pubDate>Wed, 14 Oct 2026 12:00:00 GMT</pubDate>
            </item>
            <item>
              <title>Seven Samurai (1954)</title>
              <link>https://asiandvdclub.org/download.php?id=67890.torrent</link>
              <pubDate>Wed, 14 Oct 2026 13:00:00 GMT</pubDate>
            </item>
          </channel>
        </rss>"#;

        let items = parse_rss_xml(sample);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].title, "Tokyo Story (1953) [4K Restoration]");
        assert!(items[0].torrent_url.is_some());
        assert!(items[0]
            .torrent_url
            .as_ref()
            .unwrap()
            .contains("download.php"));
        assert_eq!(items[1].title, "Seven Samurai (1954)");
        assert_eq!(
            items[1].torrent_url.as_deref(),
            Some("https://asiandvdclub.org/download.php?id=67890.torrent")
        );
    }
}
