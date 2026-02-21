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
    world: "format-plugin-v2",
    path: "../../wit/format-plugin.wit",
});

use exports::artifact_keeper::format::handler::{Guest as HandlerGuest, Metadata};
use exports::artifact_keeper::format::request_handler::{
    Guest as RequestHandlerGuest, HttpRequest, HttpResponse, RepoContext,
};

/// RPM lead magic bytes: 0xed 0xab 0xee 0xdb
const RPM_MAGIC: [u8; 4] = [0xed, 0xab, 0xee, 0xdb];

/// RPM lead is exactly 96 bytes.
const RPM_LEAD_SIZE: usize = 96;

struct RpmFormatHandler;

impl HandlerGuest for RpmFormatHandler {
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

impl RequestHandlerGuest for RpmFormatHandler {
    fn handle_request(
        request: HttpRequest,
        context: RepoContext,
        artifacts: Vec<Metadata>,
    ) -> Result<HttpResponse, String> {
        let path = request.path.as_str();

        // Only handle GET and HEAD
        if request.method != "GET" && request.method != "HEAD" {
            return Ok(HttpResponse {
                status: 405,
                headers: vec![("allow".to_string(), "GET, HEAD".to_string())],
                body: b"Method Not Allowed".to_vec(),
            });
        }

        let trimmed = path.trim_end_matches('/');

        // Route: /repodata/repomd.xml
        if trimmed == "/repodata/repomd.xml" {
            return handle_repomd_xml(&context, &artifacts);
        }

        // Route: /repodata/primary.xml.gz
        if trimmed == "/repodata/primary.xml.gz" {
            return handle_primary_xml_gz(&context, &artifacts);
        }

        // Route: /repodata/filelists.xml.gz
        if trimmed == "/repodata/filelists.xml.gz" {
            return handle_filelists_xml_gz();
        }

        // Route: /repodata/other.xml.gz
        if trimmed == "/repodata/other.xml.gz" {
            return handle_other_xml_gz();
        }

        // Route: /packages/{filename} or /Packages/{filename} - redirect to download
        if let Some(filename) = trimmed
            .strip_prefix("/packages/")
            .or_else(|| trimmed.strip_prefix("/Packages/"))
        {
            if !filename.contains('/') && !filename.is_empty() {
                return handle_package_download(filename, &context, &artifacts);
            }
        }

        // 404 for everything else
        Ok(HttpResponse {
            status: 404,
            headers: vec![("content-type".to_string(), "text/plain".to_string())],
            body: b"Not Found".to_vec(),
        })
    }
}

export!(RpmFormatHandler);

// ---------------------------------------------------------------------------
// Request handler helpers
// ---------------------------------------------------------------------------

/// Generate repomd.xml pointing to the primary, filelists, and other metadata files.
fn handle_repomd_xml(
    _context: &RepoContext,
    _artifacts: &[Metadata],
) -> Result<HttpResponse, String> {
    // Simple repomd.xml - in production you'd compute checksums of each data file,
    // but for serving purposes we use a static structure with timestamps.
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<repomd xmlns="http://linux.duke.edu/metadata/repo" xmlns:rpm="http://linux.duke.edu/metadata/rpm">
  <revision>1</revision>
  <data type="primary">
    <location href="repodata/primary.xml.gz"/>
  </data>
  <data type="filelists">
    <location href="repodata/filelists.xml.gz"/>
  </data>
  <data type="other">
    <location href="repodata/other.xml.gz"/>
  </data>
</repomd>
"#;

    Ok(HttpResponse {
        status: 200,
        headers: vec![("content-type".to_string(), "application/xml".to_string())],
        body: xml.as_bytes().to_vec(),
    })
}

/// Generate primary.xml.gz with package entries.
fn handle_primary_xml_gz(
    _context: &RepoContext,
    artifacts: &[Metadata],
) -> Result<HttpResponse, String> {
    let mut xml = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <metadata xmlns=\"http://linux.duke.edu/metadata/common\" \
         xmlns:rpm=\"http://linux.duke.edu/metadata/rpm\" \
         packages=\"",
    );
    xml.push_str(&artifacts.len().to_string());
    xml.push_str("\">\n");

    for artifact in artifacts {
        let filename = artifact.path.rsplit('/').next().unwrap_or(&artifact.path);
        let info = parse_rpm_filename(filename);

        let name = info.name.as_deref().unwrap_or("unknown");
        let version = info.version.as_deref().unwrap_or("0");
        let release = info.release.as_deref().unwrap_or("0");
        let arch = info.arch.as_deref().unwrap_or("x86_64");

        xml.push_str("  <package type=\"rpm\">\n");
        xml.push_str(&format!("    <name>{}</name>\n", xml_escape(name)));
        xml.push_str(&format!("    <arch>{}</arch>\n", xml_escape(arch)));
        xml.push_str(&format!(
            "    <version epoch=\"0\" ver=\"{}\" rel=\"{}\"/>\n",
            xml_escape(version),
            xml_escape(release)
        ));
        xml.push_str(&format!(
            "    <checksum type=\"sha256\" pkgid=\"YES\">{}</checksum>\n",
            artifact.checksum_sha256.as_deref().unwrap_or("")
        ));
        xml.push_str("    <summary/>\n");
        xml.push_str("    <description/>\n");
        xml.push_str("    <packager/>\n");
        xml.push_str("    <url/>\n");
        xml.push_str(&format!(
            "    <size package=\"{}\" installed=\"0\" archive=\"0\"/>\n",
            artifact.size_bytes
        ));
        xml.push_str(&format!(
            "    <location href=\"packages/{}\"/>\n",
            xml_escape(filename)
        ));
        xml.push_str("    <format>\n");
        xml.push_str(&format!(
            "      <rpm:provides>\n        <rpm:entry name=\"{}\" flags=\"EQ\" epoch=\"0\" ver=\"{}\" rel=\"{}\"/>\n      </rpm:provides>\n",
            xml_escape(name),
            xml_escape(version),
            xml_escape(release)
        ));
        xml.push_str("    </format>\n");
        xml.push_str("  </package>\n");
    }

    xml.push_str("</metadata>\n");

    // gzip the XML
    let compressed = gzip_compress(xml.as_bytes())?;

    Ok(HttpResponse {
        status: 200,
        headers: vec![("content-type".to_string(), "application/gzip".to_string())],
        body: compressed,
    })
}

/// Generate empty filelists.xml.gz.
fn handle_filelists_xml_gz() -> Result<HttpResponse, String> {
    let xml = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
               <filelists xmlns=\"http://linux.duke.edu/metadata/filelists\" packages=\"0\">\n\
               </filelists>\n";

    let compressed = gzip_compress(xml.as_bytes())?;

    Ok(HttpResponse {
        status: 200,
        headers: vec![("content-type".to_string(), "application/gzip".to_string())],
        body: compressed,
    })
}

/// Generate empty other.xml.gz.
fn handle_other_xml_gz() -> Result<HttpResponse, String> {
    let xml = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
               <otherdata xmlns=\"http://linux.duke.edu/metadata/other\" packages=\"0\">\n\
               </otherdata>\n";

    let compressed = gzip_compress(xml.as_bytes())?;

    Ok(HttpResponse {
        status: 200,
        headers: vec![("content-type".to_string(), "application/gzip".to_string())],
        body: compressed,
    })
}

/// Redirect package download to the artifact storage download endpoint.
fn handle_package_download(
    filename: &str,
    context: &RepoContext,
    artifacts: &[Metadata],
) -> Result<HttpResponse, String> {
    let artifact = artifacts
        .iter()
        .find(|a| a.path.rsplit('/').next().unwrap_or(&a.path) == filename);

    match artifact {
        Some(a) => {
            let download_url = format!("{}/{}", context.download_base_url, a.path);
            Ok(HttpResponse {
                status: 302,
                headers: vec![("location".to_string(), download_url)],
                body: Vec::new(),
            })
        }
        None => Ok(HttpResponse {
            status: 404,
            headers: vec![("content-type".to_string(), "text/plain".to_string())],
            body: format!("Package '{}' not found", filename).into_bytes(),
        }),
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Minimal gzip compression using the DEFLATE algorithm.
///
/// WASM plugins can't use libflate or flate2 easily, so we produce a valid
/// gzip stream with STORED blocks (no actual compression, just framing).
/// This is perfectly valid per RFC 1952 and all tools accept it.
fn gzip_compress(data: &[u8]) -> Result<Vec<u8>, String> {
    let mut output = Vec::with_capacity(data.len() + 64);

    // Gzip header (10 bytes)
    output.extend_from_slice(&[
        0x1f, 0x8b, // magic
        0x08, // method: deflate
        0x00, // flags: none
        0x00, 0x00, 0x00, 0x00, // mtime
        0x00, // extra flags
        0xff, // OS: unknown
    ]);

    // DEFLATE stored blocks
    // Each stored block can hold up to 65535 bytes
    let chunks: Vec<&[u8]> = if data.is_empty() {
        vec![&[]]
    } else {
        data.chunks(65535).collect()
    };

    for (i, chunk) in chunks.iter().enumerate() {
        let is_last = i == chunks.len() - 1;
        // Block header: 1 byte (BFINAL=1 for last, BTYPE=00 for stored)
        output.push(if is_last { 0x01 } else { 0x00 });
        let len = chunk.len() as u16;
        let nlen = !len;
        output.extend_from_slice(&len.to_le_bytes());
        output.extend_from_slice(&nlen.to_le_bytes());
        output.extend_from_slice(chunk);
    }

    // CRC32 and original size (ISIZE)
    let crc = crc32(data);
    let size = data.len() as u32;
    output.extend_from_slice(&crc.to_le_bytes());
    output.extend_from_slice(&size.to_le_bytes());

    Ok(output)
}

/// CRC32 (ISO 3309 / ITU-T V.42) used by gzip.
fn crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xEDB8_8320;
            } else {
                crc >>= 1;
            }
        }
    }
    !crc
}

/// Escape XML special characters.
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

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

    // -- handle_request (repodata) --

    fn test_context() -> RepoContext {
        RepoContext {
            repo_key: "rpm-test".to_string(),
            base_url: "http://localhost:8080/ext/rpm-custom/rpm-test".to_string(),
            download_base_url: "http://localhost:8080/api/v1/repositories/rpm-test/download"
                .to_string(),
        }
    }

    fn test_artifacts() -> Vec<Metadata> {
        vec![
            Metadata {
                path: "nginx-1.24.0-1.el9.x86_64.rpm".into(),
                version: Some("1.24.0-1.el9".into()),
                content_type: "application/x-rpm".into(),
                size_bytes: 8192,
                checksum_sha256: Some("abc123def456".into()),
            },
            Metadata {
                path: "bash-5.2.26-1.el9.x86_64.rpm".into(),
                version: Some("5.2.26-1.el9".into()),
                content_type: "application/x-rpm".into(),
                size_bytes: 4096,
                checksum_sha256: None,
            },
        ]
    }

    fn get_request(path: &str) -> HttpRequest {
        HttpRequest {
            method: "GET".to_string(),
            path: path.to_string(),
            query: String::new(),
            headers: Vec::new(),
            body: Vec::new(),
        }
    }

    #[test]
    fn handle_request_repomd_xml() {
        let resp = RpmFormatHandler::handle_request(
            get_request("/repodata/repomd.xml"),
            test_context(),
            test_artifacts(),
        )
        .unwrap();
        assert_eq!(resp.status, 200);
        let body = String::from_utf8(resp.body).unwrap();
        assert!(body.contains("<repomd"));
        assert!(body.contains("primary.xml.gz"));
        assert!(body.contains("filelists.xml.gz"));
        assert!(body.contains("other.xml.gz"));
    }

    #[test]
    fn handle_request_primary_xml_gz() {
        let resp = RpmFormatHandler::handle_request(
            get_request("/repodata/primary.xml.gz"),
            test_context(),
            test_artifacts(),
        )
        .unwrap();
        assert_eq!(resp.status, 200);
        // Verify it's valid gzip (magic bytes)
        assert!(resp.body.len() > 10);
        assert_eq!(resp.body[0], 0x1f);
        assert_eq!(resp.body[1], 0x8b);
    }

    #[test]
    fn handle_request_filelists_xml_gz() {
        let resp = RpmFormatHandler::handle_request(
            get_request("/repodata/filelists.xml.gz"),
            test_context(),
            test_artifacts(),
        )
        .unwrap();
        assert_eq!(resp.status, 200);
        assert_eq!(resp.body[0], 0x1f);
        assert_eq!(resp.body[1], 0x8b);
    }

    #[test]
    fn handle_request_other_xml_gz() {
        let resp = RpmFormatHandler::handle_request(
            get_request("/repodata/other.xml.gz"),
            test_context(),
            test_artifacts(),
        )
        .unwrap();
        assert_eq!(resp.status, 200);
        assert_eq!(resp.body[0], 0x1f);
        assert_eq!(resp.body[1], 0x8b);
    }

    #[test]
    fn handle_request_package_download_redirect() {
        let resp = RpmFormatHandler::handle_request(
            get_request("/packages/nginx-1.24.0-1.el9.x86_64.rpm"),
            test_context(),
            test_artifacts(),
        )
        .unwrap();
        assert_eq!(resp.status, 302);
        let location = resp.headers.iter().find(|(k, _)| k == "location").unwrap();
        assert!(location
            .1
            .contains("/download/nginx-1.24.0-1.el9.x86_64.rpm"));
    }

    #[test]
    fn handle_request_package_not_found() {
        let resp = RpmFormatHandler::handle_request(
            get_request("/packages/nonexistent-1.0.0-1.el9.x86_64.rpm"),
            test_context(),
            test_artifacts(),
        )
        .unwrap();
        assert_eq!(resp.status, 404);
    }

    #[test]
    fn handle_request_unknown_path() {
        let resp = RpmFormatHandler::handle_request(
            get_request("/unknown/path"),
            test_context(),
            test_artifacts(),
        )
        .unwrap();
        assert_eq!(resp.status, 404);
    }

    #[test]
    fn handle_request_post_rejected() {
        let req = HttpRequest {
            method: "POST".to_string(),
            path: "/repodata/repomd.xml".to_string(),
            query: String::new(),
            headers: Vec::new(),
            body: Vec::new(),
        };
        let resp = RpmFormatHandler::handle_request(req, test_context(), test_artifacts()).unwrap();
        assert_eq!(resp.status, 405);
    }

    // -- gzip helpers --

    #[test]
    fn gzip_compress_produces_valid_header() {
        let result = gzip_compress(b"hello").unwrap();
        assert_eq!(result[0], 0x1f);
        assert_eq!(result[1], 0x8b);
        assert_eq!(result[2], 0x08); // deflate
    }

    #[test]
    fn gzip_compress_empty_input() {
        let result = gzip_compress(b"").unwrap();
        assert!(result.len() > 10); // header + trailer at minimum
        assert_eq!(result[0], 0x1f);
        assert_eq!(result[1], 0x8b);
    }

    #[test]
    fn crc32_known_value() {
        // CRC32 of empty string is 0x00000000
        assert_eq!(crc32(b""), 0x0000_0000);
        // CRC32 of "123456789" is 0xCBF43926
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn xml_escape_special_chars() {
        assert_eq!(
            xml_escape("a<b>c&d\"e'f"),
            "a&lt;b&gt;c&amp;d&quot;e&apos;f"
        );
    }
}
