use anyhow::{bail, Context, Result};
use semver::Version;
use serde::Deserialize;
use std::path::{Path, PathBuf};

const GITHUB_REPO: &str = "nitecon/agent-tools";
const CURRENT_VERSION: &str = env!("AGENT_TOOLS_VERSION");
const CHECK_INTERVAL_SECS: u64 = 3600; // 1 hour

#[derive(Deserialize)]
struct GitHubRelease {
    tag_name: String,
    assets: Vec<GitHubAsset>,
}

#[derive(Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
}

/// Run a rate-limited auto-update check. Called on every CLI invocation.
/// Never panics, never blocks for long unless an update is available.
pub fn auto_update() {
    if std::env::var("AGENT_TOOLS_NO_UPDATE").is_ok() {
        return;
    }

    if let Err(e) = check_and_update() {
        eprintln!("[agent-tools] update check: {e}");
    }
}

/// Run a manual update check (no rate limiting). Called by `agent-tools update`.
pub fn manual_update() -> Result<()> {
    let current = Version::parse(CURRENT_VERSION).context("invalid current version")?;
    eprintln!("[agent-tools] current version: v{current}");

    let client = build_client()?;
    let release = fetch_latest_release(&client)?;

    let latest = Version::parse(release.tag_name.trim_start_matches('v'))
        .context("invalid release version")?;

    if latest <= current {
        eprintln!("[agent-tools] already up to date (v{current})");
        // Update the marker so auto_update doesn't re-check immediately
        touch_marker(&marker_path()?);
        return Ok(());
    }

    eprintln!("[agent-tools] updating: v{current} -> v{latest}");
    download_and_install(&client, &release)?;
    touch_marker(&marker_path()?);
    eprintln!("[agent-tools] updated to v{latest} — will take effect on next invocation");
    Ok(())
}

fn check_and_update() -> Result<()> {
    let marker = marker_path()?;
    if !should_check(&marker) {
        return Ok(());
    }

    let current = Version::parse(CURRENT_VERSION).context("invalid current version")?;
    let client = build_client()?;
    let release = fetch_latest_release(&client)?;

    // Always update the marker so we don't re-check immediately on failure
    touch_marker(&marker);

    let latest = Version::parse(release.tag_name.trim_start_matches('v'))
        .context("invalid release version")?;

    if latest <= current {
        return Ok(());
    }

    eprintln!("[agent-tools] update available: v{current} -> v{latest}");
    download_and_install(&client, &release)?;
    eprintln!("[agent-tools] updated to v{latest} — will take effect on next invocation");
    Ok(())
}

fn build_client() -> Result<reqwest::blocking::Client> {
    Ok(reqwest::blocking::Client::builder()
        .user_agent("agent-tools-updater")
        .timeout(std::time::Duration::from_secs(15))
        .build()?)
}

fn fetch_latest_release(client: &reqwest::blocking::Client) -> Result<GitHubRelease> {
    let url = format!("https://api.github.com/repos/{GITHUB_REPO}/releases/latest");
    Ok(client.get(&url).send()?.error_for_status()?.json()?)
}

fn download_and_install(client: &reqwest::blocking::Client, release: &GitHubRelease) -> Result<()> {
    let target = current_target()?;
    let archive_prefix = format!("agent-tools-{}-{target}", release.tag_name);

    let asset = release
        .assets
        .iter()
        .find(|a| a.name.starts_with(&archive_prefix))
        .context("no release asset for this platform")?;

    // Download to OS temp directory
    let temp_dir = std::env::temp_dir().join(format!("agent-tools-update-{}", release.tag_name));
    std::fs::create_dir_all(&temp_dir)?;

    let archive_path = temp_dir.join(&asset.name);
    let bytes = client
        .get(&asset.browser_download_url)
        .send()?
        .error_for_status()?
        .bytes()?;
    std::fs::write(&archive_path, &bytes)?;

    // Resolve actual binary location (follows symlinks)
    let exe_dir = std::env::current_exe()?
        .canonicalize()?
        .parent()
        .context("current exe has no parent directory")?
        .to_path_buf();

    extract_and_replace(&archive_path, &exe_dir)?;

    // Cleanup temp
    let _ = std::fs::remove_dir_all(&temp_dir);
    Ok(())
}

fn current_target() -> Result<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => Ok("x86_64-unknown-linux-gnu"),
        ("linux", "aarch64") => Ok("aarch64-unknown-linux-gnu"),
        ("macos", "x86_64") => Ok("x86_64-apple-darwin"),
        ("macos", "aarch64") => Ok("aarch64-apple-darwin"),
        ("windows", "x86_64") => Ok("x86_64-pc-windows-msvc"),
        ("windows", "aarch64") => Ok("aarch64-pc-windows-msvc"),
        (os, arch) => bail!("unsupported platform: {os}/{arch}"),
    }
}

/// Marker file path for rate-limiting update checks.
/// Uses `~/.agentic/.agent-tools-update-check` (persists across reboots).
fn marker_path() -> Result<PathBuf> {
    #[cfg(unix)]
    let dir = {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        PathBuf::from(home).join(".agentic")
    };

    #[cfg(windows)]
    let dir = {
        let home = std::env::var("USERPROFILE").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(home).join(".agentic")
    };

    std::fs::create_dir_all(&dir)?;
    Ok(dir.join(".agent-tools-update-check"))
}

fn should_check(marker: &Path) -> bool {
    marker
        .metadata()
        .and_then(|m| m.modified())
        .map(|t| t.elapsed().unwrap_or_default().as_secs() > CHECK_INTERVAL_SECS)
        .unwrap_or(true)
}

fn touch_marker(marker: &Path) {
    let _ = std::fs::write(marker, "");
}

/// Binary names we look for inside the release archive.
#[cfg(unix)]
const BINARY_NAMES: &[&str] = &["agent-tools-mcp", "agent-tools"];

#[cfg(windows)]
const BINARY_NAMES: &[&str] = &["agent-tools-mcp.exe", "agent-tools.exe"];

#[cfg(unix)]
fn extract_and_replace(archive: &Path, exe_dir: &Path) -> Result<()> {
    use flate2::read::GzDecoder;
    use std::os::unix::fs::PermissionsExt;
    use tar::Archive;

    let file = std::fs::File::open(archive)?;
    let decoder = GzDecoder::new(file);
    let mut archive = Archive::new(decoder);

    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.to_path_buf();
        let file_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };

        if !BINARY_NAMES.contains(&file_name.as_str()) {
            continue;
        }

        let target = exe_dir.join(&file_name);
        if !target.exists() {
            continue;
        }

        let staging = exe_dir.join(format!("{file_name}.new"));
        entry.unpack(&staging)?;

        let mut perms = std::fs::metadata(&staging)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&staging, perms)?;

        std::fs::rename(&staging, &target)
            .with_context(|| format!("failed to replace {}", target.display()))?;
    }

    Ok(())
}

#[cfg(windows)]
fn extract_and_replace(archive: &Path, exe_dir: &Path) -> Result<()> {
    use std::io::Read;

    let file = std::fs::File::open(archive)?;
    let mut zip = zip::ZipArchive::new(file)?;

    for i in 0..zip.len() {
        let mut entry = zip.by_index(i)?;
        let path = match entry.enclosed_name() {
            Some(p) => p.to_path_buf(),
            None => continue,
        };
        let file_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };

        if !BINARY_NAMES.contains(&file_name.as_str()) {
            continue;
        }

        let target = exe_dir.join(&file_name);
        if !target.exists() {
            continue;
        }

        let staging = exe_dir.join(format!("{file_name}.new"));
        let old = exe_dir.join(format!("{file_name}.old"));

        let mut buf = Vec::new();
        entry.read_to_end(&mut buf)?;
        std::fs::write(&staging, &buf)?;

        let _ = std::fs::remove_file(&old);
        std::fs::rename(&target, &old)
            .with_context(|| format!("failed to move {} to .old", target.display()))?;
        std::fs::rename(&staging, &target)
            .with_context(|| format!("failed to move .new to {}", target.display()))?;
    }

    Ok(())
}
