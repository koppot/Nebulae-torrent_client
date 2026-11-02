fn main() {
    println!("cargo:rerun-if-changed=icons/icon.ico");
    println!("cargo:rerun-if-changed=tauri.conf.json");

    #[cfg(target_os = "windows")]
    {
        let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
        if target_env == "gnu" {
            let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
            let manifest_path = out_dir.join("common_controls.manifest");
            let rc_path = out_dir.join("manifest.rc");
            let res_path = out_dir.join("manifest.o");

            let manifest_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <dependency>
    <dependentAssembly>
      <assemblyIdentity
        type="win32"
        name="Microsoft.Windows.Common-Controls"
        version="6.0.0.0"
        processorArchitecture="*"
        publicKeyToken="6595b64144ccf1df"
        language="*"
      />
    </dependentAssembly>
  </dependency>
</assembly>"#;

            let rc_content = format!(
                r#"1 24 "{}""#,
                manifest_path.display().to_string().replace('\\', "/")
            );

            if std::fs::write(&manifest_path, manifest_xml).is_ok()
                && std::fs::write(&rc_path, rc_content).is_ok()
            {
                if std::process::Command::new("windres")
                    .args([
                        "-i",
                        &rc_path.to_string_lossy(),
                        "-o",
                        &res_path.to_string_lossy(),
                    ])
                    .status()
                    .map(|s| s.success())
                    .unwrap_or(false)
                {
                    println!("cargo:rustc-link-arg={}", res_path.display());
                }
            }
        } else if target_env == "msvc" {
            println!("cargo:rustc-link-arg=/MANIFESTDEPENDENCY:type='win32' name='Microsoft.Windows.Common-Controls' version='6.0.0.0' processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'");
        }
    }

    tauri_build::build()
}
