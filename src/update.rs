//! Version label and the "is there a newer version?" check (GitHub's public API, no account needed).
//! The repository name comes from GitHub Actions at build time, so a build you made yourself
//! simply has nothing to check and says so.
use std::time::Duration;

const REPO: Option<&str> = option_env!("GITHUB_REPOSITORY");
const CHANNEL: Option<&str> = option_env!("EMU_CHANNEL");
const COMMIT: Option<&str> = option_env!("EMU_COMMIT");

/// "v0.11.0", or "v0.11.0 beta (abc1234)" for builds made by the Beta workflow.
pub fn version_label() -> String {
    let mut s = format!("v{}", env!("CARGO_PKG_VERSION"));
    if let Some(channel) = CHANNEL {
        s += &format!(" {channel}");
    }
    if let Some(sha) = COMMIT {
        s += &format!(" ({})", &sha[..sha.len().min(7)]);
    }
    s
}

/// The first string value of `"key": "value"` in some JSON text (enough for GitHub's release data).
pub fn json_str(text: &str, key: &str) -> Option<String> {
    let pat = format!("\"{key}\"");
    let mut from = 0;
    while let Some(i) = text[from..].find(&pat) {
        let after = text[from + i + pat.len()..].trim_start();
        if let Some(rest) = after.strip_prefix(':') {
            if let Some(value) = rest.trim_start().strip_prefix('"') {
                return value.find('"').map(|end| value[..end].to_string());
            }
        }
        from += i + pat.len();
    }
    None
}

fn parse_version(s: &str) -> Option<(u32, u32, u32)> {
    let mut parts = s.trim().trim_start_matches('v').split('.').map(|p| {
        p.chars().take_while(|c| c.is_ascii_digit()).collect::<String>().parse::<u32>().ok()
    });
    Some((parts.next()??, parts.next()??, parts.next().flatten().unwrap_or(0)))
}

/// Beta builds: is the published beta made from a different commit than this one?
fn decide_beta(current_commit: &str, body: &str) -> Option<(String, String)> {
    let target = json_str(body, "target_commitish")?;
    let is_sha = target.len() == 40 && target.chars().all(|c| c.is_ascii_hexdigit());
    if is_sha && !current_commit.is_empty() && target != current_commit {
        Some(("A newer beta is available".to_string(), json_str(body, "html_url")?))
    } else {
        None
    }
}

/// Normal builds: is the latest numbered release newer than this version?
fn decide_stable(current_version: &str, body: &str) -> Option<(String, String)> {
    let tag = json_str(body, "tag_name")?;
    if parse_version(&tag)? > parse_version(current_version)? {
        Some((format!("Version {tag} is available"), json_str(body, "html_url")?))
    } else {
        None
    }
}

/// Ok(Some((message, page to open))) = newer version exists. Ok(None) = up to date. Err = could not check.
pub fn check() -> Result<Option<(String, String)>, String> {
    let repo = REPO.ok_or("This build doesn't know where to look for updates.")?;
    let beta = CHANNEL == Some("beta");
    let url = if beta {
        format!("https://api.github.com/repos/{repo}/releases/tags/beta")
    } else {
        format!("https://api.github.com/repos/{repo}/releases/latest")
    };
    let resp = match ureq::get(&url)
        .set("User-Agent", "android-emulator-manager")
        .set("Accept", "application/vnd.github+json")
        .timeout(Duration::from_secs(8))
        .call()
    {
        Ok(r) => r,
        Err(ureq::Error::Status(404, _)) => return Ok(None), // no release published yet
        Err(e) => return Err(e.to_string()),
    };
    let body = resp.into_string().map_err(|e| e.to_string())?;
    Ok(if beta {
        decide_beta(COMMIT.unwrap_or(""), &body)
    } else {
        decide_stable(env!("CARGO_PKG_VERSION"), &body)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHA_A: &str = "0123456789abcdef0123456789abcdef01234567";
    const SHA_B: &str = "89abcdef0123456789abcdef0123456789abcdef";

    #[test]
    fn reads_json_fields_with_or_without_spaces() {
        let body = r#"{"url":"x","html_url": "https://github.com/u/r/releases/tag/beta","tag_name":"beta"}"#;
        assert_eq!(json_str(body, "html_url").as_deref(), Some("https://github.com/u/r/releases/tag/beta"));
        assert_eq!(json_str(body, "tag_name").as_deref(), Some("beta"));
        assert_eq!(json_str(body, "missing"), None);
    }

    #[test]
    fn compares_versions() {
        assert!(parse_version("v0.11.0") > parse_version("0.10.9"));
        assert!(parse_version("v1.0") > parse_version("0.99.99"));
        assert!(parse_version("v0.10.2") == parse_version("0.10.2"));
        assert_eq!(parse_version("beta"), None);
    }

    #[test]
    fn beta_update_is_detected_by_commit() {
        let body = format!(r#"{{"html_url":"https://example/beta","target_commitish":"{SHA_B}"}}"#);
        assert!(decide_beta(SHA_A, &body).is_some()); // newer build exists
        assert!(decide_beta(SHA_B, &body).is_none()); // this IS the newest
        assert!(decide_beta("", &body).is_none()); // unknown current commit: say nothing
        let branch = r#"{"html_url":"u","target_commitish":"main"}"#;
        assert!(decide_beta(SHA_A, branch).is_none()); // a branch name is not a commit
    }

    #[test]
    fn stable_update_is_detected_by_version() {
        let body = r#"{"tag_name":"v0.12.0","html_url":"https://example/v0.12.0"}"#;
        assert!(decide_stable("0.11.0", body).is_some());
        assert!(decide_stable("0.12.0", body).is_none());
        assert!(decide_stable("0.13.0", body).is_none());
    }
}
