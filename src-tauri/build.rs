use std::path::Path;
use std::process::Command;

fn git_output(repo: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn main() {
    let manifest_dir = std::path::PathBuf::from(
        std::env::var("CARGO_MANIFEST_DIR").expect("Cargo sets CARGO_MANIFEST_DIR"),
    );
    let repo_dir = manifest_dir.parent().unwrap_or(&manifest_dir);
    let revision = git_output(repo_dir, &["rev-parse", "HEAD"])
        .filter(|revision| !revision.is_empty())
        .unwrap_or_else(|| "unknown".to_string());
    let worktree_state = git_output(repo_dir, &["status", "--porcelain"])
        .map(|status| if status.is_empty() { "clean" } else { "dirty" })
        .unwrap_or("unknown");
    println!("cargo:rustc-env=NEBULAE_BUILD_ID={revision}-{worktree_state}");
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=../src");
    for git_path in ["HEAD", "index"] {
        if let Some(path) = git_output(repo_dir, &["rev-parse", "--git-path", git_path]) {
            println!("cargo:rerun-if-changed={}", repo_dir.join(path).display());
        }
    }

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

    #[cfg(target_os = "macos")]
    {
        println!("cargo:rustc-link-lib=framework=AVFoundation");
        println!("cargo:rustc-link-lib=framework=AVKit");
    }

    tauri_build::build()
}
