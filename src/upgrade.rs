// upgrade.rs — Self-update: fetch latest GitHub release and replace the binary.
// Cross-platform: Linux, macOS, Windows.
// Uses only reqwest (blocking) — no additional dependencies.

use anyhow::{bail, Context, Result};
use std::fs;
use std::path::PathBuf;

const REPO: &str = "IntelligenzaArtificiale/G-Type";
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Detect the asset name suffix for the current platform.
/// Must match the artifact names in release.yml.
fn platform_asset_name() -> Result<&'static str> {
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        Ok("g-type-linux-x86_64")
    }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        Ok("g-type-macos-x86_64")
    }
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        Ok("g-type-macos-aarch64")
    }
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        Ok("g-type-windows-x86_64.exe")
    }
    #[cfg(not(any(
        all(target_os = "linux", target_arch = "x86_64"),
        all(target_os = "macos", target_arch = "x86_64"),
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "windows", target_arch = "x86_64"),
    )))]
    {
        bail!("Unsupported platform for self-update. Build from source instead.");
    }
}

/// Resolve the path of the currently running binary.
fn current_binary_path() -> Result<PathBuf> {
    std::env::current_exe().context("Cannot determine path of the running binary")
}

/// Fetch the latest release tag and download URL from GitHub.
/// Returns (tag, download_url).
fn fetch_latest_release(asset_name: &str) -> Result<(String, String)> {
    let url = format!("https://api.github.com/repos/{}/releases/latest", REPO);

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .user_agent(format!("g-type/{}", CURRENT_VERSION))
        .build()
        .context("Failed to build HTTP client")?;

    let response = client
        .get(&url)
        .send()
        .context("Failed to fetch latest release from GitHub. Check your internet connection.")?;

    let status = response.status();
    if !status.is_success() {
        bail!(
            "GitHub API returned HTTP {}. No releases found at {}/releases.",
            status,
            REPO
        );
    }

    let body: serde_json::Value = response
        .json()
        .context("Failed to parse GitHub release JSON")?;

    let tag = body
        .get("tag_name")
        .and_then(|v| v.as_str())
        .context("Release JSON missing 'tag_name'")?
        .to_string();

    // Find the matching asset in the release
    let assets = body
        .get("assets")
        .and_then(|v| v.as_array())
        .context("Release JSON missing 'assets' array")?;

    let download_url = assets
        .iter()
        .find_map(|asset| {
            let name = asset.get("name")?.as_str()?;
            if name == asset_name {
                asset
                    .get("browser_download_url")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            } else {
                None
            }
        })
        .with_context(|| {
            format!(
                "Release {} does not contain asset '{}'. Available assets: {}",
                tag,
                asset_name,
                assets
                    .iter()
                    .filter_map(|a| a.get("name").and_then(|n| n.as_str()))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })?;

    Ok((tag, download_url))
}

/// Compare two semver-like version strings (e.g. "1.0.0" vs "1.1.0").
/// Returns true if `latest` is strictly newer than `current`.
fn is_newer(current: &str, latest: &str) -> bool {
    let parse = |v: &str| -> Vec<u64> {
        v.trim_start_matches('v')
            .split('.')
            .filter_map(|s| s.parse::<u64>().ok())
            .collect()
    };

    let cur = parse(current);
    let lat = parse(latest);

    for i in 0..cur.len().max(lat.len()) {
        let c = cur.get(i).copied().unwrap_or(0);
        let l = lat.get(i).copied().unwrap_or(0);
        if l > c {
            return true;
        }
        if l < c {
            return false;
        }
    }
    false
}

/// Download a file from a URL to a local path.
fn download_binary(url: &str, dest: &PathBuf) -> Result<()> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .user_agent(format!("g-type/{}", CURRENT_VERSION))
        .build()
        .context("Failed to build HTTP client for download")?;

    let response = client
        .get(url)
        .send()
        .context("Failed to download binary")?;

    let status = response.status();
    if !status.is_success() {
        bail!("Download failed with HTTP {}", status);
    }

    let bytes = response.bytes().context("Failed to read download body")?;

    fs::write(dest, &bytes)
        .with_context(|| format!("Failed to write binary to {}", dest.display()))?;

    Ok(())
}

/// Set executable permission on Unix.
#[cfg(unix)]
fn make_executable(path: &PathBuf) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(path)
        .with_context(|| format!("Cannot read metadata for {}", path.display()))?
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms)
        .with_context(|| format!("Cannot set executable permission on {}", path.display()))?;
    Ok(())
}

#[cfg(not(unix))]
fn make_executable(_path: &PathBuf) -> Result<()> {
    Ok(())
}

/// Run the self-update flow. Called from `g-type upgrade`.
pub fn run_upgrade() -> Result<()> {
    println!();
    println!("  \x1b[36m⬆  G-Type Self-Upgrade\x1b[0m");
    println!("  Current version: \x1b[1mv{}\x1b[0m", CURRENT_VERSION);
    println!();

    // Step 1: Detect platform
    let asset_name = platform_asset_name()?;

    // Step 2: Fetch latest release info
    print!("  Checking for updates... ");
    let (tag, download_url) = fetch_latest_release(asset_name)?;
    let latest_version = tag.trim_start_matches('v');
    println!("\x1b[32m✔\x1b[0m");

    // Step 3: Compare versions
    if !is_newer(CURRENT_VERSION, &tag) {
        println!(
            "  \x1b[32m✔ Already up to date!\x1b[0m (v{})",
            CURRENT_VERSION
        );
        println!();
        return Ok(());
    }

    println!(
        "  New version available: \x1b[1;33mv{}\x1b[0m → \x1b[1;32mv{}\x1b[0m",
        CURRENT_VERSION, latest_version
    );

    // Step 4: Determine install path
    let current_path = current_binary_path()?;
    println!("  Binary location: {}", current_path.display());

    // Step 5: Download to a temporary file next to the current binary
    let tmp_path = current_path.with_extension("upgrade-tmp");
    print!("  Downloading v{}... ", latest_version);
    download_binary(&download_url, &tmp_path)?;
    make_executable(&tmp_path)?;
    println!("\x1b[32m✔\x1b[0m");

    // Step 6: Atomic-ish replace.
    //   - On Unix: rename old → .bak, rename new → current, remove .bak
    //   - On Windows: rename old → .bak, rename new → current
    //     (Windows can rename a running exe, just can't delete it)
    let bak_path = current_path.with_extension("bak");

    // Remove any previous backup
    let _ = fs::remove_file(&bak_path);

    print!("  Replacing binary... ");
    fs::rename(&current_path, &bak_path).with_context(|| {
        format!(
            "Cannot move current binary to backup. Try running with sudo/admin privileges.\n  Source: {}\n  Dest: {}",
            current_path.display(),
            bak_path.display()
        )
    })?;

    if let Err(e) = fs::rename(&tmp_path, &current_path) {
        // Rollback: restore the backup
        let _ = fs::rename(&bak_path, &current_path);
        let _ = fs::remove_file(&tmp_path);
        return Err(e).context("Failed to install new binary (rolled back to previous version)");
    }

    // Clean up backup (best-effort — on Windows the old exe may still be locked)
    let _ = fs::remove_file(&bak_path);
    // Clean up tmp if it somehow still exists
    let _ = fs::remove_file(&tmp_path);

    println!("\x1b[32m✔\x1b[0m");

    println!();
    println!(
        "  \x1b[32m✔ Successfully upgraded to v{}!\x1b[0m",
        latest_version
    );
    println!("  Restart G-Type to use the new version.");
    println!();

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_newer_patch() {
        assert!(is_newer("1.0.0", "v1.0.1"));
    }

    #[test]
    fn test_is_newer_minor() {
        assert!(is_newer("1.0.0", "v1.1.0"));
    }

    #[test]
    fn test_is_newer_major() {
        assert!(is_newer("1.0.0", "v2.0.0"));
    }

    #[test]
    fn test_is_newer_same() {
        assert!(!is_newer("1.0.0", "v1.0.0"));
    }

    #[test]
    fn test_is_newer_older() {
        assert!(!is_newer("1.1.0", "v1.0.0"));
    }

    #[test]
    fn test_is_newer_no_prefix() {
        assert!(is_newer("1.0.0", "1.1.0"));
    }

    #[test]
    fn test_platform_asset_name() {
        // Should return Ok on any supported CI/dev platform
        let result = platform_asset_name();
        assert!(result.is_ok(), "Platform should be supported: {:?}", result);
        let name = result.unwrap();
        assert!(name.starts_with("g-type-"));
    }

    #[test]
    fn test_current_binary_path() {
        let path = current_binary_path();
        assert!(path.is_ok());
    }
}
