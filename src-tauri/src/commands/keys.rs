use crate::utils::keyring::{get_secret, store_secret, trakt_client_id_target};
use serde::{Deserialize, Serialize};

/// Store the Trakt client ID in the OS keychain
#[derive(Debug, Deserialize)]
pub struct StoreTraktClientIdRequest {
    pub client_id: String,
}

#[tauri::command]
pub fn store_trakt_client_id(client_id: String) -> Result<(), String> {
    store_secret(&trakt_client_id_target(), &client_id)
}

/// Retrieve the Trakt client ID from the OS keychain
#[tauri::command]
pub fn get_trakt_client_id() -> Result<Option<String>, String> {
    get_secret(&trakt_client_id_target()).map(Some).or(Ok(None))
}

/// Test Trakt API connection with the stored Client ID
#[derive(Debug, Serialize)]
pub struct TraktTestResult {
    pub success: bool,
    pub message: String,
}

#[tauri::command]
pub fn test_trakt_connection() -> Result<TraktTestResult, String> {
    let client_id = match get_secret(&trakt_client_id_target()) {
        Ok(id) if !id.is_empty() => id,
        _ => {
            return Ok(TraktTestResult {
                success: false,
                message: "No Trakt Client ID stored in keychain".to_string(),
            });
        }
    };

    let client = reqwest::blocking::Client::new();
    let url = "https://api.trakt.tv/search/movie,show?query=test&extended=full&limit=1";

    match client
        .get(url)
        .header("trakt-api-key", &client_id)
        .header("trakt-api-version", "2")
        .send()
    {
        Ok(resp) if resp.status().is_success() => Ok(TraktTestResult {
            success: true,
            message: "Trakt API connection successful".to_string(),
        }),
        Ok(resp) => Ok(TraktTestResult {
            success: false,
            message: format!("Trakt API returned status: {}", resp.status()),
        }),
        Err(e) => Ok(TraktTestResult {
            success: false,
            message: format!("Trakt API request failed: {}", e),
        }),
    }
}
