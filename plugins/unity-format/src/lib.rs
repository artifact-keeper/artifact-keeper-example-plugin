//! Unity Package Format Plugin for Artifact Keeper
//!
//! Handles `.unitypackage` files, which are gzipped tarballs containing Unity assets.
//! This serves as a realistic example of building a custom format handler plugin.
//!
//! ## Format structure
//!
//! A `.unitypackage` is a gzipped tar archive with entries like:
//! ```text
//! <guid>/
//! <guid>/asset         — the actual file
//! <guid>/asset.meta    — Unity metadata YAML
//! <guid>/pathname      — text file with the asset path inside the Unity project
//! ```

wit_bindgen::generate!({
    world: "format-plugin",
    path: "../../wit/format-plugin.wit",
});

use exports::artifact_keeper::format::handler::{Guest, Metadata};

struct UnityFormatHandler;

impl Guest for UnityFormatHandler {
    fn format_key() -> String {
        "unity".to_string()
    }

    fn parse_metadata(path: String, data: Vec<u8>) -> Result<Metadata, String> {
        if data.is_empty() {
            return Err("Empty file".to_string());
        }

        let version = extract_version_from_path(&path);
        let is_gzip = data.len() >= 2 && data[0] == 0x1f && data[1] == 0x8b;

        let content_type = if is_gzip {
            "application/gzip"
        } else {
            "application/octet-stream"
        };

        Ok(Metadata {
            path,
            version,
            content_type: content_type.to_string(),
            size_bytes: data.len() as u64,
            checksum_sha256: None, // Host calculates SHA-256
        })
    }

    fn validate(path: String, data: Vec<u8>) -> Result<(), String> {
        if data.is_empty() {
            return Err("Unity package cannot be empty".to_string());
        }

        if path.is_empty() {
            return Err("Artifact path cannot be empty".to_string());
        }

        // Verify the path ends with .unitypackage
        if !path.to_lowercase().ends_with(".unitypackage") {
            return Err(format!(
                "Expected .unitypackage extension, got: {}",
                path.rsplit('/').next().unwrap_or(&path)
            ));
        }

        // Verify gzip magic bytes (0x1f 0x8b)
        if data.len() < 2 {
            return Err("File too small to be a valid gzip archive".to_string());
        }

        if data[0] != 0x1f || data[1] != 0x8b {
            return Err(format!(
                "Invalid gzip header: expected [1f, 8b], got [{:02x}, {:02x}]",
                data[0], data[1]
            ));
        }

        // Verify gzip compression method byte (0x08 = deflate)
        if data.len() >= 3 && data[2] != 0x08 {
            return Err(format!(
                "Unsupported gzip compression method: {:02x} (expected 08/deflate)",
                data[2]
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
                let mut entry = serde_json::Map::new();
                entry.insert("path".into(), serde_json::Value::String(a.path.clone()));
                if let Some(ref v) = a.version {
                    entry.insert("version".into(), serde_json::Value::String(v.clone()));
                }
                entry.insert(
                    "size_bytes".into(),
                    serde_json::Value::Number(a.size_bytes.into()),
                );
                entry.insert(
                    "content_type".into(),
                    serde_json::Value::String(a.content_type.clone()),
                );
                serde_json::Value::Object(entry)
            })
            .collect();

        let index = serde_json::json!({
            "format": "unity",
            "total_count": artifacts.len(),
            "total_size_bytes": artifacts.iter().map(|a| a.size_bytes).sum::<u64>(),
            "packages": entries,
        });

        let json_bytes = serde_json::to_vec_pretty(&index)
            .map_err(|e| format!("Failed to serialize index: {e}"))?;

        Ok(Some(vec![("unity-index.json".to_string(), json_bytes)]))
    }
}

export!(UnityFormatHandler);

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Extract version from path components like `com/example/MyPlugin/1.2.3/MyPlugin-1.2.3.unitypackage`
fn extract_version_from_path(path: &str) -> Option<String> {
    for part in path.split('/').rev() {
        if is_semver_like(part) {
            return Some(part.to_string());
        }
    }

    // Try filename: `MyPlugin-1.2.3.unitypackage` or `MyPlugin-3.0.0-beta.unitypackage`
    let filename = path.rsplit('/').next()?;
    let stem = filename
        .strip_suffix(".unitypackage")
        .or_else(|| filename.rsplit_once('.').map(|(s, _)| s))?;

    // Find the first hyphen followed by a digit — that starts the version
    for (i, _) in stem.match_indices('-') {
        let candidate = &stem[i + 1..];
        if candidate.starts_with(|c: char| c.is_ascii_digit()) && is_semver_like(candidate) {
            return Some(candidate.to_string());
        }
    }

    None
}

fn is_semver_like(s: &str) -> bool {
    let s = s.strip_prefix('v').unwrap_or(s);
    if !s.starts_with(|c: char| c.is_ascii_digit()) || !s.contains('.') {
        return false;
    }
    s.chars()
        .all(|c| c.is_ascii_digit() || c == '.' || c == '-' || c.is_ascii_alphabetic())
}

// ---------------------------------------------------------------------------
// Tests — run with: cargo test --target <host-target>
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_key_is_unity() {
        assert_eq!(UnityFormatHandler::format_key(), "unity");
    }

    #[test]
    fn parse_metadata_detects_gzip() {
        let data = vec![0x1f, 0x8b, 0x08, 0x00, 0x00];
        let result =
            UnityFormatHandler::parse_metadata("assets/MyPlugin-1.0.0.unitypackage".into(), data);
        let meta = result.unwrap();
        assert_eq!(meta.content_type, "application/gzip");
        assert_eq!(meta.version, Some("1.0.0".to_string()));
    }

    #[test]
    fn parse_metadata_non_gzip() {
        let data = vec![0x50, 0x4b, 0x03, 0x04]; // ZIP header
        let result = UnityFormatHandler::parse_metadata("assets/thing.unitypackage".into(), data);
        let meta = result.unwrap();
        assert_eq!(meta.content_type, "application/octet-stream");
    }

    #[test]
    fn validate_accepts_valid_gzip() {
        let data = vec![0x1f, 0x8b, 0x08, 0x00, 0x00];
        let result = UnityFormatHandler::validate("MyPlugin-1.0.0.unitypackage".into(), data);
        assert!(result.is_ok());
    }

    #[test]
    fn validate_rejects_empty() {
        let result = UnityFormatHandler::validate("test.unitypackage".into(), vec![]);
        assert!(result.unwrap_err().contains("empty"));
    }

    #[test]
    fn validate_rejects_wrong_extension() {
        let data = vec![0x1f, 0x8b, 0x08];
        let result = UnityFormatHandler::validate("test.zip".into(), data);
        assert!(result.unwrap_err().contains(".unitypackage"));
    }

    #[test]
    fn validate_rejects_bad_magic() {
        let data = vec![0x50, 0x4b, 0x03];
        let result = UnityFormatHandler::validate("test.unitypackage".into(), data);
        assert!(result.unwrap_err().contains("Invalid gzip header"));
    }

    #[test]
    fn generate_index_empty() {
        let result = UnityFormatHandler::generate_index(vec![]);
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn generate_index_produces_json() {
        let artifacts = vec![Metadata {
            path: "MyPlugin-1.0.0.unitypackage".into(),
            version: Some("1.0.0".into()),
            content_type: "application/gzip".into(),
            size_bytes: 1024,
            checksum_sha256: None,
        }];
        let result = UnityFormatHandler::generate_index(artifacts)
            .unwrap()
            .unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].0, "unity-index.json");
        let json: serde_json::Value = serde_json::from_slice(&result[0].1).unwrap();
        assert_eq!(json["total_count"], 1);
        assert_eq!(json["format"], "unity");
    }

    #[test]
    fn version_from_path_component() {
        assert_eq!(
            extract_version_from_path("com/example/plugin/2.1.0/plugin-2.1.0.unitypackage"),
            Some("2.1.0".to_string())
        );
    }

    #[test]
    fn version_from_filename() {
        assert_eq!(
            extract_version_from_path("MyPlugin-3.0.0-beta.unitypackage"),
            Some("3.0.0-beta".to_string())
        );
    }

    #[test]
    fn no_version() {
        assert_eq!(extract_version_from_path("MyPlugin.unitypackage"), None);
    }
}
