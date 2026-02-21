//! RPM Package Format Plugin for Artifact Keeper
//!
//! Handles `.rpm` files used by Red Hat, Fedora, SUSE, and other RPM-based Linux distributions.
//! This plugin demonstrates binary format validation (RPM lead magic bytes) and right-to-left
//! filename parsing to extract structured metadata from RPM naming conventions.
//!
//! ## RPM filename convention
//!
//! ```text
//! name-version-release.arch.rpm
//! ```
//!
//! Examples:
//! - `nginx-1.24.0-1.el9.x86_64.rpm`
//! - `python3-numpy-1.24.2-4.el9.x86_64.rpm` (name contains hyphens)
//! - `bash-completion-2.11-5.el9.noarch.rpm`

wit_bindgen::generate!({
    world: "format-plugin",
    path: "../../wit/format-plugin.wit",
});

use exports::artifact_keeper::format::handler::{Guest, Metadata};

/// RPM lead magic bytes: 0xed 0xab 0xee 0xdb
const RPM_MAGIC: [u8; 4] = [0xed, 0xab, 0xee, 0xdb];

/// RPM lead is exactly 96 bytes.
const RPM_LEAD_SIZE: usize = 96;

struct RpmFormatHandler;

impl Guest for RpmFormatHandler {
    fn format_key() -> String {
        "rpm-custom".to_string()
    }

    fn parse_metadata(path: String, data: Vec<u8>) -> Result<Metadata, String> {
        if data.is_empty() {
            return Err("Empty file".to_string());
        }

        let has_rpm_magic = data.len() >= 4 && data[..4] == RPM_MAGIC;

        let content_type = if has_rpm_magic {
            "application/x-rpm"
        } else {
            "application/octet-stream"
        };

        let version = extract_version_from_rpm_filename(&path);

        Ok(Metadata {
            path,
            version,
            content_type: content_type.to_string(),
            size_bytes: data.len() as u64,
            checksum_sha256: None,
        })
    }

    fn validate(path: String, data: Vec<u8>) -> Result<(), String> {
        if data.is_empty() {
            return Err("RPM package cannot be empty".to_string());
        }

        if path.is_empty() {
            return Err("Artifact path cannot be empty".to_string());
        }

        // Verify .rpm extension
        if !path.to_lowercase().ends_with(".rpm") {
            return Err(format!(
                "Expected .rpm extension, got: {}",
                path.rsplit('/').next().unwrap_or(&path)
            ));
        }

        // RPM lead is 96 bytes minimum
        if data.len() < RPM_LEAD_SIZE {
            return Err(format!(
                "File too small for RPM lead: {} bytes (minimum {})",
                data.len(),
                RPM_LEAD_SIZE
            ));
        }

        // Verify RPM magic bytes
        if data[..4] != RPM_MAGIC {
            return Err(format!(
                "Invalid RPM magic: expected [ed, ab, ee, db], got [{:02x}, {:02x}, {:02x}, {:02x}]",
                data[0], data[1], data[2], data[3]
            ));
        }

        Ok(())
    }

    fn generate_index(artifacts: Vec<Metadata>) -> Result<Option<Vec<(String, Vec<u8>)>>, String> {
        if artifacts.is_empty() {
            return Ok(None);
        }

        let entries: Vec<serde_json::Value> = artifacts
            .iter()
            .map(|a| {
                let filename = a.path.rsplit('/').next().unwrap_or(&a.path);
                let info = parse_rpm_filename(filename);

                let mut entry = serde_json::Map::new();
                entry.insert("path".into(), serde_json::Value::String(a.path.clone()));
                if let Some(ref v) = a.version {
                    entry.insert("version".into(), serde_json::Value::String(v.clone()));
                }
                if let Some(name) = info.name {
                    entry.insert("name".into(), serde_json::Value::String(name));
                }
                if let Some(arch) = info.arch {
                    entry.insert("arch".into(), serde_json::Value::String(arch));
                }
                if let Some(release) = info.release {
                    entry.insert("release".into(), serde_json::Value::String(release));
                }
                entry.insert(
                    "size_bytes".into(),
                    serde_json::Value::Number(a.size_bytes.into()),
                );
                serde_json::Value::Object(entry)
            })
            .collect();

        let index = serde_json::json!({
            "format": "rpm-custom",
            "total_count": artifacts.len(),
            "total_size_bytes": artifacts.iter().map(|a| a.size_bytes).sum::<u64>(),
            "packages": entries,
        });

        let json_bytes = serde_json::to_vec_pretty(&index)
            .map_err(|e| format!("Failed to serialize index: {e}"))?;

        Ok(Some(vec![("rpm-index.json".to_string(), json_bytes)]))
    }
}

export!(RpmFormatHandler);

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

struct RpmFileInfo {
    name: Option<String>,
    version: Option<String>,
    release: Option<String>,
    arch: Option<String>,
}

/// Parse an RPM filename into its components.
///
/// RPM filenames follow the convention: `name-version-release.arch.rpm`
/// The name can contain hyphens, so we parse right-to-left:
/// 1. Strip `.rpm` extension
/// 2. Split on last `.` to get arch
/// 3. Split remainder on last `-` to get release
/// 4. Split remainder on last `-` to get version (rest is name)
fn parse_rpm_filename(filename: &str) -> RpmFileInfo {
    let stem = match filename.strip_suffix(".rpm") {
        Some(s) => s,
        None => {
            return RpmFileInfo {
                name: None,
                version: None,
                release: None,
                arch: None,
            }
        }
    };

    // Split on last dot for arch: "nginx-1.24.0-1.el9.x86_64" -> ("nginx-1.24.0-1.el9", "x86_64")
    let (before_arch, arch) = match stem.rsplit_once('.') {
        Some((b, a)) => (b, Some(a.to_string())),
        None => (stem, None),
    };

    // Split on last hyphen for release: "nginx-1.24.0-1.el9" -> ("nginx-1.24.0", "1.el9")
    let (before_release, release) = match before_arch.rsplit_once('-') {
        Some((b, r)) => (b, Some(r.to_string())),
        None => (before_arch, None),
    };

    // Split on last hyphen for version: "nginx-1.24.0" -> ("nginx", "1.24.0")
    let (name, version) = match before_release.rsplit_once('-') {
        Some((n, v)) => (Some(n.to_string()), Some(v.to_string())),
        None => (Some(before_release.to_string()), None),
    };

    RpmFileInfo {
        name,
        version,
        release,
        arch,
    }
}

/// Extract the version string from an RPM filename in a path.
fn extract_version_from_rpm_filename(path: &str) -> Option<String> {
    let filename = path.rsplit('/').next()?;
    let info = parse_rpm_filename(filename);

    match (info.version, info.release) {
        (Some(ver), Some(rel)) => Some(format!("{ver}-{rel}")),
        (Some(ver), None) => Some(ver),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- format_key --

    #[test]
    fn format_key_is_rpm() {
        assert_eq!(RpmFormatHandler::format_key(), "rpm-custom");
    }

    // -- RPM filename parsing --

    #[test]
    fn parse_simple_rpm() {
        let info = parse_rpm_filename("nginx-1.24.0-1.el9.x86_64.rpm");
        assert_eq!(info.name.as_deref(), Some("nginx"));
        assert_eq!(info.version.as_deref(), Some("1.24.0"));
        assert_eq!(info.release.as_deref(), Some("1.el9"));
        assert_eq!(info.arch.as_deref(), Some("x86_64"));
    }

    #[test]
    fn parse_rpm_with_hyphens_in_name() {
        let info = parse_rpm_filename("python3-numpy-1.24.2-4.el9.x86_64.rpm");
        assert_eq!(info.name.as_deref(), Some("python3-numpy"));
        assert_eq!(info.version.as_deref(), Some("1.24.2"));
        assert_eq!(info.release.as_deref(), Some("4.el9"));
        assert_eq!(info.arch.as_deref(), Some("x86_64"));
    }

    #[test]
    fn parse_rpm_noarch() {
        let info = parse_rpm_filename("bash-completion-2.11-5.el9.noarch.rpm");
        assert_eq!(info.name.as_deref(), Some("bash-completion"));
        assert_eq!(info.version.as_deref(), Some("2.11"));
        assert_eq!(info.release.as_deref(), Some("5.el9"));
        assert_eq!(info.arch.as_deref(), Some("noarch"));
    }

    #[test]
    fn parse_rpm_no_extension() {
        let info = parse_rpm_filename("not-an-rpm.txt");
        assert!(info.name.is_none());
    }

    // -- version extraction from path --

    #[test]
    fn version_from_simple_filename() {
        assert_eq!(
            extract_version_from_rpm_filename("Packages/nginx-1.24.0-1.el9.x86_64.rpm"),
            Some("1.24.0-1.el9".to_string())
        );
    }

    #[test]
    fn version_from_hyphenated_name() {
        assert_eq!(
            extract_version_from_rpm_filename("python3-numpy-1.24.2-4.el9.x86_64.rpm"),
            Some("1.24.2-4.el9".to_string())
        );
    }

    // -- parse_metadata --

    #[test]
    fn parse_metadata_detects_rpm_magic() {
        let mut data = vec![0; RPM_LEAD_SIZE];
        data[..4].copy_from_slice(&RPM_MAGIC);
        let result =
            RpmFormatHandler::parse_metadata("Packages/nginx-1.24.0-1.el9.x86_64.rpm".into(), data);
        let meta = result.unwrap();
        assert_eq!(meta.content_type, "application/x-rpm");
        assert_eq!(meta.version, Some("1.24.0-1.el9".to_string()));
    }

    #[test]
    fn parse_metadata_non_rpm_content() {
        let data = vec![0x50, 0x4b, 0x03, 0x04]; // ZIP magic
        let result = RpmFormatHandler::parse_metadata("test.rpm".into(), data);
        let meta = result.unwrap();
        assert_eq!(meta.content_type, "application/octet-stream");
    }

    #[test]
    fn parse_metadata_empty_error() {
        let result = RpmFormatHandler::parse_metadata("test.rpm".into(), vec![]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Empty"));
    }

    // -- validate --

    #[test]
    fn validate_accepts_valid_rpm() {
        let mut data = vec![0; RPM_LEAD_SIZE];
        data[..4].copy_from_slice(&RPM_MAGIC);
        let result = RpmFormatHandler::validate("test.rpm".into(), data);
        assert!(result.is_ok());
    }

    #[test]
    fn validate_rejects_empty() {
        let result = RpmFormatHandler::validate("test.rpm".into(), vec![]);
        assert!(result.unwrap_err().contains("empty"));
    }

    #[test]
    fn validate_rejects_wrong_extension() {
        let mut data = vec![0; RPM_LEAD_SIZE];
        data[..4].copy_from_slice(&RPM_MAGIC);
        let result = RpmFormatHandler::validate("test.deb".into(), data);
        assert!(result.unwrap_err().contains(".rpm"));
    }

    #[test]
    fn validate_rejects_too_small() {
        let data = RPM_MAGIC.to_vec(); // Only 4 bytes, need 96
        let result = RpmFormatHandler::validate("test.rpm".into(), data);
        assert!(result.unwrap_err().contains("too small"));
    }

    #[test]
    fn validate_rejects_bad_magic() {
        let data = vec![0; RPM_LEAD_SIZE];
        let result = RpmFormatHandler::validate("test.rpm".into(), data);
        assert!(result.unwrap_err().contains("Invalid RPM magic"));
    }

    #[test]
    fn validate_rejects_empty_path() {
        let mut data = vec![0; RPM_LEAD_SIZE];
        data[..4].copy_from_slice(&RPM_MAGIC);
        let result = RpmFormatHandler::validate("".into(), data);
        assert!(result.unwrap_err().contains("path"));
    }

    // -- generate_index --

    #[test]
    fn generate_index_empty() {
        let result = RpmFormatHandler::generate_index(vec![]);
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn generate_index_produces_json() {
        let artifacts = vec![
            Metadata {
                path: "Packages/nginx-1.24.0-1.el9.x86_64.rpm".into(),
                version: Some("1.24.0-1.el9".into()),
                content_type: "application/x-rpm".into(),
                size_bytes: 8192,
                checksum_sha256: None,
            },
            Metadata {
                path: "Packages/bash-5.2.26-1.el9.x86_64.rpm".into(),
                version: Some("5.2.26-1.el9".into()),
                content_type: "application/x-rpm".into(),
                size_bytes: 4096,
                checksum_sha256: None,
            },
        ];
        let result = RpmFormatHandler::generate_index(artifacts)
            .unwrap()
            .unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].0, "rpm-index.json");

        let json: serde_json::Value = serde_json::from_slice(&result[0].1).unwrap();
        assert_eq!(json["format"], "rpm-custom");
        assert_eq!(json["total_count"], 2);
        assert_eq!(json["total_size_bytes"], 12288);

        let packages = json["packages"].as_array().unwrap();
        assert_eq!(packages[0]["name"], "nginx");
        assert_eq!(packages[0]["arch"], "x86_64");
    }
}
