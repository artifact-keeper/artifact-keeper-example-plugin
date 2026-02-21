//! Python Package (PyPI) Format Plugin for Artifact Keeper
//!
//! Handles Python wheels (`.whl`) and source distributions (`.tar.gz`, `.zip`).
//! This plugin demonstrates filename convention parsing following PEP 427 (wheels)
//! and PEP 503 (Simple Repository API) standards.
//!
//! ## Wheel filename convention (PEP 427)
//!
//! ```text
//! {distribution}-{version}(-{build})?-{python}-{abi}-{platform}.whl
//! ```
//!
//! Examples:
//! - `requests-2.28.0-py3-none-any.whl`
//! - `numpy-1.24.2-cp311-cp311-manylinux_2_17_x86_64.whl`
//!
//! ## Source distribution convention
//!
//! ```text
//! {name}-{version}.tar.gz
//! {name}-{version}.zip
//! ```

wit_bindgen::generate!({
    world: "format-plugin",
    path: "../../wit/format-plugin.wit",
});

use exports::artifact_keeper::format::handler::{Guest, Metadata};

struct PypiFormatHandler;

impl Guest for PypiFormatHandler {
    fn format_key() -> String {
        "pypi".to_string()
    }

    fn parse_metadata(path: String, data: Vec<u8>) -> Result<Metadata, String> {
        if data.is_empty() {
            return Err("Empty file".to_string());
        }

        let filename = path.rsplit('/').next().unwrap_or(&path);
        let version = extract_version(filename);

        let content_type = if filename.ends_with(".whl") || filename.ends_with(".zip") {
            "application/zip"
        } else if filename.ends_with(".tar.gz") {
            "application/gzip"
        } else {
            "application/octet-stream"
        };

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
            return Err("Python package cannot be empty".to_string());
        }

        if path.is_empty() {
            return Err("Artifact path cannot be empty".to_string());
        }

        let filename = path.rsplit('/').next().unwrap_or(&path);
        let lower = filename.to_lowercase();

        if !lower.ends_with(".whl") && !lower.ends_with(".tar.gz") && !lower.ends_with(".zip") {
            return Err(format!(
                "Expected .whl, .tar.gz, or .zip extension, got: {filename}"
            ));
        }

        // Validate wheel filename structure (PEP 427)
        if let Some(stem) = lower.strip_suffix(".whl") {
            let parts: Vec<&str> = stem.split('-').collect();
            if parts.len() < 5 {
                return Err(format!(
                    "Invalid wheel filename: expected at least 5 dash-separated parts \
                     (name-version-python-abi-platform), got {} in '{filename}'",
                    parts.len()
                ));
            }
        }

        // Validate source distribution has a version separator
        let sdist_stem = lower
            .strip_suffix(".tar.gz")
            .or_else(|| lower.strip_suffix(".zip"));
        if let Some(stem) = sdist_stem {
            if !stem.contains('-') {
                return Err(format!(
                    "Invalid source distribution filename: expected 'name-version' format, \
                     got '{stem}'"
                ));
            }
        }

        Ok(())
    }

    fn generate_index(artifacts: Vec<Metadata>) -> Result<Option<Vec<(String, Vec<u8>)>>, String> {
        if artifacts.is_empty() {
            return Ok(None);
        }

        // Collect unique normalized package names
        let mut packages: Vec<String> = artifacts
            .iter()
            .filter_map(|a| {
                let filename = a.path.rsplit('/').next()?;
                extract_package_name(filename).map(|n| normalize_package_name(&n))
            })
            .collect();
        packages.sort();
        packages.dedup();

        // Generate PEP 503 Simple Repository root index
        let mut html = String::from(
            "<!DOCTYPE html>\n<html>\n<head><title>Simple Index</title></head>\n<body>\n",
        );
        for pkg in &packages {
            html.push_str(&format!("  <a href=\"/simple/{pkg}/\">{pkg}</a>\n"));
        }
        html.push_str("</body>\n</html>\n");

        // Also generate a JSON index for API consumers
        let entries: Vec<serde_json::Value> = artifacts
            .iter()
            .map(|a| {
                let filename = a.path.rsplit('/').next().unwrap_or(&a.path);
                let name = extract_package_name(filename)
                    .map(|n| normalize_package_name(&n))
                    .unwrap_or_default();

                let mut entry = serde_json::Map::new();
                entry.insert("path".into(), serde_json::Value::String(a.path.clone()));
                entry.insert("name".into(), serde_json::Value::String(name));
                if let Some(ref v) = a.version {
                    entry.insert("version".into(), serde_json::Value::String(v.clone()));
                }
                entry.insert(
                    "content_type".into(),
                    serde_json::Value::String(a.content_type.clone()),
                );
                entry.insert(
                    "size_bytes".into(),
                    serde_json::Value::Number(a.size_bytes.into()),
                );
                serde_json::Value::Object(entry)
            })
            .collect();

        let json_index = serde_json::json!({
            "format": "pypi",
            "total_count": artifacts.len(),
            "total_size_bytes": artifacts.iter().map(|a| a.size_bytes).sum::<u64>(),
            "packages": entries,
        });

        let json_bytes = serde_json::to_vec_pretty(&json_index)
            .map_err(|e| format!("Failed to serialize index: {e}"))?;

        Ok(Some(vec![
            ("simple/index.html".to_string(), html.into_bytes()),
            ("pypi-index.json".to_string(), json_bytes),
        ]))
    }
}

export!(PypiFormatHandler);

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Normalize a Python package name per PEP 503.
///
/// Converts to lowercase and replaces any run of non-alphanumeric characters
/// with a single hyphen.
fn normalize_package_name(name: &str) -> String {
    let lower = name.to_lowercase();
    let mut result = String::with_capacity(lower.len());
    let mut prev_was_separator = false;

    for ch in lower.chars() {
        if ch.is_ascii_alphanumeric() {
            prev_was_separator = false;
            result.push(ch);
        } else if !prev_was_separator {
            prev_was_separator = true;
            result.push('-');
        }
    }

    // Strip leading/trailing hyphens
    result.trim_matches('-').to_string()
}

/// Extract the package name from a filename.
fn extract_package_name(filename: &str) -> Option<String> {
    if let Some(stem) = filename.strip_suffix(".whl") {
        // Wheel: first dash-separated part is the distribution name
        stem.split('-').next().map(|s| s.to_string())
    } else if let Some(stem) = filename.strip_suffix(".tar.gz") {
        // Split on last hyphen: everything before is the name
        stem.rsplit_once('-').map(|(name, _)| name.to_string())
    } else if let Some(stem) = filename.strip_suffix(".zip") {
        stem.rsplit_once('-').map(|(name, _)| name.to_string())
    } else {
        None
    }
}

/// Extract version from a Python package filename.
fn extract_version(filename: &str) -> Option<String> {
    if let Some(stem) = filename.strip_suffix(".whl") {
        // Wheel: second dash-separated part is the version
        let parts: Vec<&str> = stem.split('-').collect();
        if parts.len() >= 2 {
            Some(parts[1].to_string())
        } else {
            None
        }
    } else if let Some(stem) = filename.strip_suffix(".tar.gz") {
        stem.rsplit_once('-').map(|(_, ver)| ver.to_string())
    } else if let Some(stem) = filename.strip_suffix(".zip") {
        stem.rsplit_once('-').map(|(_, ver)| ver.to_string())
    } else {
        None
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
    fn format_key_is_pypi() {
        assert_eq!(PypiFormatHandler::format_key(), "pypi");
    }

    // -- package name normalization (PEP 503) --

    #[test]
    fn normalize_simple_name() {
        assert_eq!(normalize_package_name("requests"), "requests");
    }

    #[test]
    fn normalize_underscores() {
        assert_eq!(normalize_package_name("My_Package"), "my-package");
    }

    #[test]
    fn normalize_dots() {
        assert_eq!(normalize_package_name("some.package"), "some-package");
    }

    #[test]
    fn normalize_consecutive_separators() {
        assert_eq!(normalize_package_name("Package__Name"), "package-name");
    }

    #[test]
    fn normalize_mixed_separators() {
        assert_eq!(normalize_package_name("My.Cool_Package"), "my-cool-package");
    }

    #[test]
    fn normalize_leading_trailing() {
        assert_eq!(normalize_package_name("_leading_"), "leading");
    }

    // -- wheel filename parsing --

    #[test]
    fn extract_name_from_wheel() {
        assert_eq!(
            extract_package_name("requests-2.28.0-py3-none-any.whl"),
            Some("requests".to_string())
        );
    }

    #[test]
    fn extract_version_from_wheel() {
        assert_eq!(
            extract_version("requests-2.28.0-py3-none-any.whl"),
            Some("2.28.0".to_string())
        );
    }

    #[test]
    fn extract_version_from_wheel_with_build_tag() {
        assert_eq!(
            extract_version("package-1.0.0-1-cp39-cp39-manylinux1_x86_64.whl"),
            Some("1.0.0".to_string())
        );
    }

    // -- source distribution parsing --

    #[test]
    fn extract_name_from_sdist() {
        assert_eq!(
            extract_package_name("requests-2.28.0.tar.gz"),
            Some("requests".to_string())
        );
    }

    #[test]
    fn extract_name_from_sdist_with_hyphens() {
        assert_eq!(
            extract_package_name("my-cool-package-1.0.0.tar.gz"),
            Some("my-cool-package".to_string())
        );
    }

    #[test]
    fn extract_version_from_sdist() {
        assert_eq!(
            extract_version("requests-2.28.0.tar.gz"),
            Some("2.28.0".to_string())
        );
    }

    #[test]
    fn extract_version_from_zip() {
        assert_eq!(
            extract_version("my-package-1.0.0.zip"),
            Some("1.0.0".to_string())
        );
    }

    // -- parse_metadata --

    #[test]
    fn parse_metadata_wheel() {
        let data = vec![0x50, 0x4b, 0x03, 0x04]; // ZIP magic
        let result = PypiFormatHandler::parse_metadata(
            "packages/requests/2.28.0/requests-2.28.0-py3-none-any.whl".into(),
            data,
        );
        let meta = result.unwrap();
        assert_eq!(meta.content_type, "application/zip");
        assert_eq!(meta.version, Some("2.28.0".to_string()));
    }

    #[test]
    fn parse_metadata_sdist() {
        let data = vec![0x1f, 0x8b, 0x08]; // gzip magic
        let result = PypiFormatHandler::parse_metadata(
            "packages/requests/2.28.0/requests-2.28.0.tar.gz".into(),
            data,
        );
        let meta = result.unwrap();
        assert_eq!(meta.content_type, "application/gzip");
        assert_eq!(meta.version, Some("2.28.0".to_string()));
    }

    #[test]
    fn parse_metadata_empty_error() {
        let result = PypiFormatHandler::parse_metadata("test.whl".into(), vec![]);
        assert!(result.is_err());
    }

    // -- validate --

    #[test]
    fn validate_accepts_wheel() {
        let data = vec![0x50, 0x4b, 0x03, 0x04];
        let result = PypiFormatHandler::validate("requests-2.28.0-py3-none-any.whl".into(), data);
        assert!(result.is_ok());
    }

    #[test]
    fn validate_accepts_sdist() {
        let data = vec![0x1f, 0x8b, 0x08];
        let result = PypiFormatHandler::validate("requests-2.28.0.tar.gz".into(), data);
        assert!(result.is_ok());
    }

    #[test]
    fn validate_rejects_empty() {
        let result = PypiFormatHandler::validate("test.whl".into(), vec![]);
        assert!(result.unwrap_err().contains("empty"));
    }

    #[test]
    fn validate_rejects_wrong_extension() {
        let result = PypiFormatHandler::validate("test.rpm".into(), vec![0x00]);
        assert!(result.unwrap_err().contains(".whl"));
    }

    #[test]
    fn validate_rejects_bad_wheel_filename() {
        let data = vec![0x50, 0x4b];
        let result = PypiFormatHandler::validate("bad-name.whl".into(), data);
        assert!(result.unwrap_err().contains("5 dash-separated"));
    }

    #[test]
    fn validate_rejects_sdist_without_version() {
        let data = vec![0x1f, 0x8b];
        let result = PypiFormatHandler::validate("noversion.tar.gz".into(), data);
        assert!(result.unwrap_err().contains("name-version"));
    }

    #[test]
    fn validate_rejects_empty_path() {
        let result = PypiFormatHandler::validate("".into(), vec![0x00]);
        assert!(result.unwrap_err().contains("path"));
    }

    // -- generate_index --

    #[test]
    fn generate_index_empty() {
        let result = PypiFormatHandler::generate_index(vec![]);
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn generate_index_produces_html_and_json() {
        let artifacts = vec![
            Metadata {
                path: "packages/requests/2.28.0/requests-2.28.0-py3-none-any.whl".into(),
                version: Some("2.28.0".into()),
                content_type: "application/zip".into(),
                size_bytes: 2048,
                checksum_sha256: None,
            },
            Metadata {
                path: "packages/numpy/1.24.2/numpy-1.24.2.tar.gz".into(),
                version: Some("1.24.2".into()),
                content_type: "application/gzip".into(),
                size_bytes: 4096,
                checksum_sha256: None,
            },
        ];
        let result = PypiFormatHandler::generate_index(artifacts)
            .unwrap()
            .unwrap();
        assert_eq!(result.len(), 2);

        // HTML index
        assert_eq!(result[0].0, "simple/index.html");
        let html = String::from_utf8(result[0].1.clone()).unwrap();
        assert!(html.contains("numpy"));
        assert!(html.contains("requests"));
        assert!(html.contains("/simple/"));

        // JSON index
        assert_eq!(result[1].0, "pypi-index.json");
        let json: serde_json::Value = serde_json::from_slice(&result[1].1).unwrap();
        assert_eq!(json["format"], "pypi");
        assert_eq!(json["total_count"], 2);
    }

    #[test]
    fn generate_index_normalizes_names() {
        let artifacts = vec![Metadata {
            path: "packages/My_Package-1.0.0-py3-none-any.whl".into(),
            version: Some("1.0.0".into()),
            content_type: "application/zip".into(),
            size_bytes: 1024,
            checksum_sha256: None,
        }];
        let result = PypiFormatHandler::generate_index(artifacts)
            .unwrap()
            .unwrap();
        let html = String::from_utf8(result[0].1.clone()).unwrap();
        assert!(html.contains("my-package"));
    }
}
