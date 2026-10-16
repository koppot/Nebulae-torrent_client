use std::collections::HashMap;

/// Resolve the poster image URLs the ADC tracker already displays for its
/// torrents, keyed by torrent name.
///
/// The torrents.php page embeds each row's poster in an OpenTip attribute
/// (`data-ot="<img src='…' title='…'">`). One authenticated fetch of that
/// page therefore yields the whole name → poster mapping for the Media
/// view and the hover popups, without any site-side changes. Results are
/// cached client-side (frontend localStorage) and refreshed on demand.
#[tauri::command]
pub async fn fetch_adc_poster_map() -> Result<HashMap<String, String>, String> {
    tokio::task::spawn_blocking(|| {
        let html = fetch_adc_html("https://asiandvdclub.org/torrents.php")?;
        Ok(extract_poster_map(&html))
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

/// Download an ADC poster image into the app's hidden cache directory and
/// return the file path (owner directive: posters live in the app cache,
/// NOT next to the media). Needed because `/poster/{id}` is hotlink-
/// protected (Referer check): an <img> tag from the webview would be
/// rejected, while an authenticated backend GET works. Local files also
/// survive offline.
#[tauri::command]
pub async fn download_adc_poster(url: String) -> Result<String, String> {
    tokio::task::spawn_blocking(move || {
        let parsed = url::Url::parse(&url).map_err(|e| format!("url: {e}"))?;
        let host = parsed.host_str().unwrap_or_default().to_lowercase();
        if !matches!(host.as_str(), "asiandvdclub.org" | "www.asiandvdclub.org") {
            return Err(format!("not an ADC url: {host}"));
        }

        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .map_err(|e| format!("client: {e}"))?;
        let mut req = client.get(&url);
        if let Ok(cookie) = crate::utils::keyring::get_secret("adc:session_cookie") {
            if !cookie.is_empty() {
                req = req.header("Cookie", cookie);
            }
        }
        let bytes = req
            .send()
            .map_err(|e| format!("fetch: {e}"))?
            .error_for_status()
            .map_err(|e| format!("status: {e}"))?
            .bytes()
            .map_err(|e| format!("bytes: {e}"))?;
        if bytes.is_empty() {
            return Err("empty poster body".into());
        }

        let ext = if bytes.starts_with(&[0x89, b'P']) {
            "png"
        } else {
            "webp"
        };

        // App's hidden cache directory (stable FNV-1a name from the URL so
        // repeat resolutions dedupe to one file).
        let cache_dir = crate::utils::paths::AppPaths::init()?
            .cache_dir
            .join("posters");
        std::fs::create_dir_all(&cache_dir).map_err(|e| format!("mkdir: {e}"))?;
        let mut hash: u64 = 0xcbf29ce484222325;
        for b in url.as_bytes() {
            hash ^= *b as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
        let path = cache_dir.join(format!("{hash:016x}.{ext}"));
        std::fs::write(&path, &bytes).map_err(|e| format!("write: {e}"))?;
        Ok(path.display().to_string())
    })
    .await
    .map_err(|e| format!("join: {e}"))?
}

/// Mirrors `fetch_adc_page`'s fetch discipline: ADC hosts only, website
/// session cookie from the keychain (the announce passkey is NOT a website
/// credential and is never sent), bounded timeout, login-gate detection.
fn fetch_adc_html(url: &str) -> Result<String, String> {
    let parsed = url::Url::parse(url).map_err(|e| format!("url: {e}"))?;
    let host = parsed.host_str().unwrap_or_default().to_lowercase();
    if !matches!(host.as_str(), "asiandvdclub.org" | "www.asiandvdclub.org") {
        return Err(format!("not an ADC url: {host}"));
    }

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("client: {e}"))?;
    let mut req = client.get(url);
    if let Ok(cookie) = crate::utils::keyring::get_secret("adc:session_cookie") {
        if !cookie.is_empty() {
            req = req.header("Cookie", cookie);
        }
    }
    let html = req
        .send()
        .map_err(|e| format!("fetch: {e}"))?
        .text()
        .map_err(|e| format!("body: {e}"))?;

    let lower = html.to_lowercase();
    if lower.contains("name=\"form_login\"") || lower.contains("login.php") && html.len() < 4096 {
        return Err("ADC page requires a website login".into());
    }
    Ok(html)
}

/// The attribute always ends with  `title='…'>&nbsp;"`; use that full
/// closing sequence as the title delimiter so embedded apostrophes
/// (represented by `&#039;` followed by a literal `'`) don't truncate.
fn extract_poster_map(html: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let mut rest = html;
    while let Some(pos) = rest.find("data-ot=\"<img src='") {
        rest = &rest[pos + "data-ot=\"<img src='".len()..];
        let Some(src_end) = rest.find('\'') else {
            break;
        };
        let src = &rest[..src_end];
        let Some(title_marker) = rest.find("title='") else {
            break;
        };
        let after_title = &rest[title_marker + "title='".len()..];
        // The attribute always ends with `'>&nbsp;"` — use that as the
        // unambiguous title delimiter so `'` in the title text is fine.
        let closing = "'>&nbsp;\"";
        let Some(title_end) = after_title.find(closing) else {
            break;
        };
        let title = after_title[..title_end].trim().to_string();
        if !src.is_empty() && !title.is_empty() {
            let src = html_escape_decode(src);
            let title = html_escape_decode(&title);
            map.entry(title).or_insert(src);
        }
        // Advance past this attribute's closing sequence.
        let Some(next) = rest.find(">&nbsp;\"") else {
            break;
        };
        rest = &rest[next..];
    }
    map
}

/// Decode the handful of entities ADC emits inside attributes.
fn html_escape_decode(s: &str) -> String {
    s.replace("&#039;", "'")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&quot;", "\"")
        .replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_poster_pairs_and_decodes_entities() {
        let html = r#"<tr><td><span id="id_101" data-ot="<img src='/poster/101' title='Foo: Bar (1999)'>&nbsp;" data-ot-group="torrents"></span></td></tr>
        <tr><td><span id="id_102" data-ot="<img src='https://example.org/x.webp' title='Baz &#039;Qux&#039; (2001)'>&nbsp;" data-ot-group="torrents"></span></td></tr>"#;
        let map = extract_poster_map(html);
        assert_eq!(
            map.get("Foo: Bar (1999)").map(String::as_str),
            Some("/poster/101")
        );
        assert_eq!(
            map.get("Baz 'Qux' (2001)").map(String::as_str),
            Some("https://example.org/x.webp")
        );
        assert_eq!(map.len(), 2);
    }

    #[test]
    fn returns_empty_for_page_without_posters() {
        assert!(extract_poster_map("<html><body>nothing here</body></html>").is_empty());
    }
}
