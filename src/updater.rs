//! Startup update check against GitHub Releases API.
//!
//! Compares the running version against the latest release on GitHub. This is
//! a lightweight, non-blocking check that runs once at startup.

use serde::Deserialize;
use tracing::{debug, info};

use crate::error::VaaniError;

/// GitHub repository in `owner/repo` format.
const GITHUB_REPO: &str = "anthropics/vaani";

/// Current application version from Cargo.toml.
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Response shape from the GitHub Releases API (only fields we need).
#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    html_url: String,
}

/// Result of an update check.
#[derive(Debug, Clone)]
pub struct UpdateStatus {
    pub current: String,
    pub latest: String,
    pub update_available: bool,
    pub release_url: String,
}

/// Check for updates by querying the GitHub Releases API.
///
/// Returns `Ok(None)` if the check fails gracefully (no network, rate-limited,
/// etc.) — update checks should never block or crash the app.
pub async fn check_for_update(
    client: &reqwest::Client,
) -> Result<Option<UpdateStatus>, VaaniError> {
    let url = format!("https://api.github.com/repos/{GITHUB_REPO}/releases/latest");

    let response = match client
        .get(&url)
        .header("User-Agent", format!("Vaani/{CURRENT_VERSION}"))
        .header("Accept", "application/vnd.github.v3+json")
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
    {
        Ok(resp) => resp,
        Err(e) => {
            debug!("Update check failed (network): {e}");
            return Ok(None);
        }
    };

    if !response.status().is_success() {
        debug!(
            status = %response.status(),
            "Update check failed (HTTP status)"
        );
        return Ok(None);
    }

    let release: GitHubRelease = match response.json().await {
        Ok(r) => r,
        Err(e) => {
            debug!("Update check failed (parse): {e}");
            return Ok(None);
        }
    };

    let latest = release.tag_name.trim_start_matches('v').to_string();
    let update_available = is_newer(&latest, CURRENT_VERSION);

    if update_available {
        info!(
            current = CURRENT_VERSION,
            latest = %latest,
            url = %release.html_url,
            "Update available"
        );
    } else {
        debug!(
            current = CURRENT_VERSION,
            latest = %latest,
            "Already on latest version"
        );
    }

    Ok(Some(UpdateStatus {
        current: CURRENT_VERSION.to_string(),
        latest,
        update_available,
        release_url: release.html_url,
    }))
}

/// Simple semver comparison: returns true if `latest` is newer than `current`.
fn is_newer(latest: &str, current: &str) -> bool {
    let parse =
        |v: &str| -> Vec<u32> { v.split('.').filter_map(|s| s.parse::<u32>().ok()).collect() };

    let l = parse(latest);
    let c = parse(current);

    // Compare component by component
    for i in 0..l.len().max(c.len()) {
        let lv = l.get(i).copied().unwrap_or(0);
        let cv = c.get(i).copied().unwrap_or(0);
        match lv.cmp(&cv) {
            std::cmp::Ordering::Greater => return true,
            std::cmp::Ordering::Less => return false,
            std::cmp::Ordering::Equal => continue,
        }
    }

    false
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_newer_detects_patch_update() {
        assert!(is_newer("0.1.1", "0.1.0"));
    }

    #[test]
    fn is_newer_detects_minor_update() {
        assert!(is_newer("0.2.0", "0.1.0"));
    }

    #[test]
    fn is_newer_detects_major_update() {
        assert!(is_newer("1.0.0", "0.9.9"));
    }

    #[test]
    fn is_newer_same_version_is_not_newer() {
        assert!(!is_newer("0.1.0", "0.1.0"));
    }

    #[test]
    fn is_newer_older_is_not_newer() {
        assert!(!is_newer("0.0.9", "0.1.0"));
    }

    #[test]
    fn is_newer_handles_different_lengths() {
        assert!(is_newer("0.1.0.1", "0.1.0"));
        assert!(!is_newer("0.1.0", "0.1.0.1"));
    }

    #[test]
    fn current_version_is_set() {
        assert!(!CURRENT_VERSION.is_empty());
        assert!(
            CURRENT_VERSION.contains('.'),
            "should be semver: {CURRENT_VERSION}"
        );
    }

    #[test]
    fn github_repo_is_set() {
        assert!(
            GITHUB_REPO.contains('/'),
            "should be owner/repo: {GITHUB_REPO}"
        );
    }

    #[test]
    fn update_status_debug_format() {
        let status = UpdateStatus {
            current: "0.1.0".into(),
            latest: "0.2.0".into(),
            update_available: true,
            release_url: "https://example.com".into(),
        };
        let debug_str = format!("{status:?}");
        assert!(debug_str.contains("update_available: true"));
    }
}
