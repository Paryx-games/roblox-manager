use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::PathBuf,
    time::Duration,
};

use serde::Deserialize;
use sha2::{Digest, Sha256};
use thiserror::Error;

const REPOSITORY: &str = "Paryx-games/roblox-manager";
const CHECKSUM_ASSET: &str = "SHA256SUMS.txt";
const MAX_RELEASE_METADATA_BYTES: usize = 1024 * 1024;
const MAX_CHECKSUM_BYTES: usize = 64 * 1024;
const MAX_INSTALLER_BYTES: usize = 128 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum UpdateError {
    #[error("The requested update version is invalid or no longer available.")]
    InvalidVersion,
    #[error("The update release could not be retrieved.")]
    ReleaseRequest,
    #[error("The update release does not contain the expected installer and checksum.")]
    MissingAsset,
    #[error("The update download failed or exceeded its size limit.")]
    Download,
    #[error("The update checksum is missing or invalid.")]
    InvalidChecksum,
    #[error("The update installer failed to stage locally.")]
    StageFile,
}

#[derive(Debug)]
pub struct StagedInstaller {
    pub path: PathBuf,
    pub sha256: String,
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<ReleaseAsset>,
}

#[derive(Deserialize)]
struct ReleaseAsset {
    name: String,
    browser_download_url: String,
}

fn release_tag(version: &str) -> Option<String> {
    let version = version.strip_prefix('v').unwrap_or(version);
    let parts = version.split('.').collect::<Vec<_>>();
    if parts.len() != 3
        || parts
            .iter()
            .any(|part| part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()))
    {
        return None;
    }
    Some(format!("v{version}"))
}

fn checksum_for_asset(manifest: &str, filename: &str) -> Option<String> {
    let mut matching = manifest.lines().filter_map(|line| {
        let mut fields = line.split_whitespace();
        let digest = fields.next()?;
        let name = fields.next()?.trim_start_matches('*');
        (name == filename
            && digest.len() == 64
            && digest.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .then(|| digest.to_ascii_lowercase())
    });
    let checksum = matching.next()?;
    matching.next().is_none().then_some(checksum)
}

fn digest_hex(digest: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        let _ = write!(output, "{byte:02x}");
    }
    output
}

fn release_matches_version(release: &Release, tag: &str) -> bool {
    release.tag_name == tag && !release.draft && !release.prerelease
}

fn update_client() -> Result<reqwest::Client, UpdateError> {
    reqwest::Client::builder()
        .user_agent("RM-update-installer")
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(300))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            let url = attempt.url();
            let allowed = url.scheme() == "https"
                && matches!(
                    url.host_str(),
                    Some("api.github.com")
                        | Some("github.com")
                        | Some("release-assets.githubusercontent.com")
                        | Some("objects.githubusercontent.com")
                );
            if allowed && attempt.previous().len() < 5 {
                attempt.follow()
            } else {
                attempt.stop()
            }
        }))
        .build()
        .map_err(|_| UpdateError::ReleaseRequest)
}

async fn download_bounded(
    client: &reqwest::Client,
    url: &str,
    max_bytes: usize,
) -> Result<Vec<u8>, UpdateError> {
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|_| UpdateError::Download)?
        .error_for_status()
        .map_err(|_| UpdateError::Download)?;
    response_bytes_bounded(response, max_bytes).await
}

async fn response_bytes_bounded(
    mut response: reqwest::Response,
    max_bytes: usize,
) -> Result<Vec<u8>, UpdateError> {
    if response
        .content_length()
        .is_some_and(|length| length > max_bytes as u64)
    {
        return Err(UpdateError::Download);
    }
    let mut contents = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| UpdateError::Download)? {
        if contents.len().saturating_add(chunk.len()) > max_bytes {
            return Err(UpdateError::Download);
        }
        contents.extend_from_slice(&chunk);
    }
    Ok(contents)
}

pub async fn stage_release_installer(
    version: &str,
    current_version: &str,
) -> Result<StagedInstaller, crate::CoreError> {
    let tag = release_tag(version).ok_or(UpdateError::InvalidVersion)?;
    if !crate::api::is_newer_stable_release(&tag, current_version) {
        return Err(UpdateError::InvalidVersion.into());
    }

    let client = update_client()?;
    let release_url = format!("https://api.github.com/repos/{REPOSITORY}/releases/tags/{tag}");
    let response = client
        .get(release_url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|_| UpdateError::ReleaseRequest)?
        .error_for_status()
        .map_err(|_| UpdateError::ReleaseRequest)?;
    let release_bytes = response_bytes_bounded(response, MAX_RELEASE_METADATA_BYTES)
        .await
        .map_err(|_| UpdateError::ReleaseRequest)?;
    let release: Release =
        serde_json::from_slice(&release_bytes).map_err(|_| UpdateError::ReleaseRequest)?;

    if !release_matches_version(&release, &tag) {
        return Err(UpdateError::InvalidVersion.into());
    }
    let filename = format!("roblox-manager-{tag}-windows-x64-setup.exe");
    let expected_installer_url =
        format!("https://github.com/{REPOSITORY}/releases/download/{tag}/{filename}");
    let expected_checksum_url =
        format!("https://github.com/{REPOSITORY}/releases/download/{tag}/{CHECKSUM_ASSET}");
    let installer_url = release
        .assets
        .iter()
        .find(|asset| {
            asset.name == filename && asset.browser_download_url == expected_installer_url
        })
        .map(|asset| asset.browser_download_url.as_str())
        .ok_or(UpdateError::MissingAsset)?;
    let checksum_url = release
        .assets
        .iter()
        .find(|asset| {
            asset.name == CHECKSUM_ASSET && asset.browser_download_url == expected_checksum_url
        })
        .map(|asset| asset.browser_download_url.as_str())
        .ok_or(UpdateError::MissingAsset)?;

    let checksum_bytes = download_bounded(&client, checksum_url, MAX_CHECKSUM_BYTES).await?;
    let checksum_manifest =
        std::str::from_utf8(&checksum_bytes).map_err(|_| UpdateError::InvalidChecksum)?;
    let expected_checksum =
        checksum_for_asset(checksum_manifest, &filename).ok_or(UpdateError::InvalidChecksum)?;

    let installer = download_bounded(&client, installer_url, MAX_INSTALLER_BYTES).await?;
    if installer.is_empty() {
        return Err(UpdateError::Download.into());
    }
    let actual_checksum = crate::assets::sha256_hex(&installer);
    if actual_checksum != expected_checksum {
        return Err(UpdateError::InvalidChecksum.into());
    }

    let path = std::env::temp_dir().join(format!("rm-update-{}.exe", uuid::Uuid::new_v4()));
    let write_path = path.clone();
    match tokio::task::spawn_blocking(move || -> io::Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&write_path)?;
        let result = file.write_all(&installer).and_then(|()| file.sync_all());
        drop(file);
        if result.is_err() {
            let _ = fs::remove_file(&write_path);
        }
        result
    })
    .await
    {
        Ok(Ok(())) => {}
        Ok(Err(_)) => return Err(UpdateError::StageFile.into()),
        Err(_) => {
            let _ = fs::remove_file(&path);
            return Err(UpdateError::StageFile.into());
        }
    }

    Ok(StagedInstaller {
        path,
        sha256: actual_checksum,
    })
}

pub fn verify_sha256_file(path: &std::path::Path, expected: &str) -> io::Result<bool> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(digest_hex(&hasher.finalize()) == expected)
}

pub fn remove_staged_installer(path: &std::path::Path) -> io::Result<()> {
    fs::remove_file(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_tags_only_accept_stable_numeric_versions() {
        assert_eq!(release_tag("2.5.0").as_deref(), Some("v2.5.0"));
        assert_eq!(release_tag("v2.5.0").as_deref(), Some("v2.5.0"));
        for version in ["", "2.5", "2.5.0/other", "2.5.0-rc.1", "2.5.0.1"] {
            assert!(release_tag(version).is_none());
        }
    }

    #[test]
    fn release_validation_rejects_drafts_prereleases_and_wrong_tags() {
        let release: Release = serde_json::from_str(
            r#"{"tag_name":"v2.5.0","draft":false,"prerelease":false,"assets":[]}"#,
        )
        .unwrap();
        assert!(release_matches_version(&release, "v2.5.0"));

        let draft: Release = serde_json::from_str(
            r#"{"tag_name":"v2.5.0","draft":true,"prerelease":false,"assets":[]}"#,
        )
        .unwrap();
        let prerelease: Release = serde_json::from_str(
            r#"{"tag_name":"v2.5.0","draft":false,"prerelease":true,"assets":[]}"#,
        )
        .unwrap();
        assert!(!release_matches_version(&draft, "v2.5.0"));
        assert!(!release_matches_version(&prerelease, "v2.5.0"));
        assert!(!release_matches_version(&release, "v2.5.1"));
    }

    #[test]
    fn checksum_manifest_requires_one_valid_entry_for_the_installer() {
        let checksum = "a".repeat(64);
        let manifest = format!("{checksum}  setup.exe\n");
        assert_eq!(
            checksum_for_asset(&manifest, "setup.exe").as_deref(),
            Some(checksum.as_str())
        );
        assert!(checksum_for_asset(&manifest, "other.exe").is_none());
        assert!(
            checksum_for_asset(&format!("{manifest}{checksum}  setup.exe\n"), "setup.exe")
                .is_none()
        );
        assert!(checksum_for_asset("not-a-hash  setup.exe\n", "setup.exe").is_none());
    }

    #[test]
    fn staged_file_hash_verification_detects_changed_content() {
        let path =
            std::env::temp_dir().join(format!("rm-update-test-{}.bin", uuid::Uuid::new_v4()));
        fs::write(&path, b"synthetic installer").unwrap();
        let checksum = crate::assets::sha256_hex(b"synthetic installer");
        assert!(verify_sha256_file(&path, &checksum).unwrap());
        assert!(!verify_sha256_file(&path, &"0".repeat(64)).unwrap());
        fs::remove_file(path).unwrap();
    }
}
