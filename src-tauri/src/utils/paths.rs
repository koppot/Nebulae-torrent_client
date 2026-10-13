use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub base_data_dir: PathBuf,
    pub internal_dir: PathBuf,
    pub logs_dir: PathBuf,
    pub diagnostics_dir: PathBuf,
    pub db_dir: PathBuf,
    pub db_file: PathBuf,
    pub cache_dir: PathBuf,
    pub engine_dir: PathBuf,
    pub debug_dir: PathBuf,
}

impl AppPaths {
    pub fn init() -> Result<Self, String> {
        // Resolve platform standard app data directory for "Nebulae"
        let data_dir = dirs::data_dir()
            .ok_or_else(|| "Failed to resolve platform standard data directory".to_string())?
            .join("Nebulae");

        let internal_dir = data_dir.join("internal");
        let logs_dir = internal_dir.join("logs");
        let diagnostics_dir = internal_dir.join("diagnostics");
        let db_dir = internal_dir.join("db");
        let db_file = db_dir.join("nebulae.db");
        let cache_dir = internal_dir.join("cache");
        let engine_dir = internal_dir.join("engine-state");
        let debug_dir = internal_dir.join("debug");

        // Ensure all internal isolated directories exist
        fs::create_dir_all(&logs_dir).map_err(|e| format!("Failed to create logs dir: {e}"))?;
        fs::create_dir_all(&diagnostics_dir)
            .map_err(|e| format!("Failed to create diagnostics dir: {e}"))?;
        fs::create_dir_all(&db_dir).map_err(|e| format!("Failed to create db dir: {e}"))?;
        fs::create_dir_all(&cache_dir).map_err(|e| format!("Failed to create cache dir: {e}"))?;
        fs::create_dir_all(&engine_dir).map_err(|e| format!("Failed to create engine dir: {e}"))?;
        fs::create_dir_all(&debug_dir).map_err(|e| format!("Failed to create debug dir: {e}"))?;

        Ok(Self {
            base_data_dir: data_dir,
            internal_dir,
            logs_dir,
            diagnostics_dir,
            db_dir,
            db_file,
            cache_dir,
            engine_dir,
            debug_dir,
        })
    }
}
