use crate::hashing::{hex_bytes, Sha256Hasher};
use crate::models::SourceClass;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{Read, Result as IoResult};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const DEFAULT_MAX_INTAKE_BYTES: u64 = 100 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QuarantineStatus {
    Pending,
    Accepted,
    RejectedOversized,
    RejectedUnreadable,
    Parsed,
    Sanitized,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntakeItem {
    pub intake_id: String,
    pub source_id: String,
    pub source_class: SourceClass,
    pub raw_path: String,
    pub raw_sha256: String,
    pub imported_at: u64,
    pub mime_type: String,
    pub size_bytes: u64,
    pub dirty_flag: bool,
    pub quarantine_status: QuarantineStatus,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct IntakeOptions {
    pub max_size_bytes: u64,
}

impl Default for IntakeOptions {
    fn default() -> Self {
        Self {
            max_size_bytes: DEFAULT_MAX_INTAKE_BYTES,
        }
    }
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub fn is_dirty_source(source_class: SourceClass) -> bool {
    matches!(source_class, SourceClass::DirtyPublicData | SourceClass::UnverifiedDump)
}

pub fn sha256_file(path: &Path) -> IoResult<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256Hasher::new();
    let mut buffer = [0_u8; 64 * 1024];

    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }

    Ok(hasher.finalize_hex())
}

pub fn detect_mime(path: &Path) -> String {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default()
        .to_lowercase()
        .as_str()
    {
        "txt" | "log" => "text/plain",
        "csv" => "text/csv",
        "json" | "jsonl" => "application/json",
        "html" | "htm" => "text/html",
        "xml" => "application/xml",
        "pdf" => "application/pdf",
        "zip" => "application/zip",
        "7z" => "application/x-7z-compressed",
        "rar" => "application/vnd.rar",
        "tar" => "application/x-tar",
        "gz" => "application/gzip",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "mp4" => "video/mp4",
        "mp3" => "audio/mpeg",
        _ => "application/octet-stream",
    }
    .to_string()
}

fn make_intake_id(source_id: &str, raw_sha256: &str, imported_at: u64) -> String {
    let mut hasher = Sha256Hasher::new();
    hasher.update(format!("{}::{}::{}", source_id, raw_sha256, imported_at).as_bytes());
    let hash = hex_bytes(&hasher.finalize());
    format!("intake_{}", &hash[..16])
}

pub fn build_intake_item(
    source_id: &str,
    source_class: SourceClass,
    raw_path: impl AsRef<Path>,
    options: IntakeOptions,
) -> IntakeItem {
    let path: PathBuf = raw_path.as_ref().to_path_buf();
    let imported_at = now_unix();
    let mime_type = detect_mime(&path);
    let dirty_flag = is_dirty_source(source_class);
    let mut notes = Vec::new();

    let metadata = match std::fs::metadata(&path) {
        Ok(metadata) => metadata,
        Err(err) => {
            notes.push(format!("metadata_error={}", err));
            return IntakeItem {
                intake_id: make_intake_id(source_id, "unreadable", imported_at),
                source_id: source_id.to_string(),
                source_class,
                raw_path: path.to_string_lossy().to_string(),
                raw_sha256: String::new(),
                imported_at,
                mime_type,
                size_bytes: 0,
                dirty_flag,
                quarantine_status: QuarantineStatus::RejectedUnreadable,
                notes,
            };
        }
    };

    let size_bytes = metadata.len();
    if size_bytes > options.max_size_bytes {
        notes.push(format!(
            "file_size_exceeds_limit:{}>{}",
            size_bytes, options.max_size_bytes
        ));
        return IntakeItem {
            intake_id: make_intake_id(source_id, "oversized", imported_at),
            source_id: source_id.to_string(),
            source_class,
            raw_path: path.to_string_lossy().to_string(),
            raw_sha256: String::new(),
            imported_at,
            mime_type,
            size_bytes,
            dirty_flag,
            quarantine_status: QuarantineStatus::RejectedOversized,
            notes,
        };
    }

    let raw_sha256 = match sha256_file(&path) {
        Ok(hash) => hash,
        Err(err) => {
            notes.push(format!("sha256_error={}", err));
            return IntakeItem {
                intake_id: make_intake_id(source_id, "hash-error", imported_at),
                source_id: source_id.to_string(),
                source_class,
                raw_path: path.to_string_lossy().to_string(),
                raw_sha256: String::new(),
                imported_at,
                mime_type,
                size_bytes,
                dirty_flag,
                quarantine_status: QuarantineStatus::RejectedUnreadable,
                notes,
            };
        }
    };

    IntakeItem {
        intake_id: make_intake_id(source_id, &raw_sha256, imported_at),
        source_id: source_id.to_string(),
        source_class,
        raw_path: path.to_string_lossy().to_string(),
        raw_sha256,
        imported_at,
        mime_type,
        size_bytes,
        dirty_flag,
        quarantine_status: QuarantineStatus::Accepted,
        notes,
    }
}

pub fn mark_status(item: &mut IntakeItem, status: QuarantineStatus, note: impl Into<String>) {
    item.quarantine_status = status;
    let note = note.into();
    if !note.is_empty() {
        item.notes.push(note);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp_file(name: &str, content: &[u8]) -> PathBuf {
        let mut path = std::env::temp_dir();
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        path.push(format!("xgen_intake_{}_{}", unique, name));
        let mut file = File::create(&path).expect("create temp file");
        file.write_all(content).expect("write temp file");
        path
    }

    #[test]
    fn detects_common_mime_from_extension() {
        assert_eq!(detect_mime(Path::new("data.json")), "application/json");
        assert_eq!(detect_mime(Path::new("chat.csv")), "text/csv");
        assert_eq!(detect_mime(Path::new("archive.zip")), "application/zip");
    }

    #[test]
    fn accepted_item_has_hash_and_size() {
        let path = temp_file("sample.txt", b"hello intake");
        let item = build_intake_item(
            "local_import",
            SourceClass::LocalImport,
            &path,
            IntakeOptions::default(),
        );
        assert_eq!(item.quarantine_status, QuarantineStatus::Accepted);
        assert_eq!(item.size_bytes, 12);
        assert!(!item.raw_sha256.is_empty());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn oversized_file_is_rejected() {
        let path = temp_file("oversized.bin", b"1234567890");
        let item = build_intake_item(
            "local_import",
            SourceClass::LocalImport,
            &path,
            IntakeOptions { max_size_bytes: 3 },
        );
        assert_eq!(item.quarantine_status, QuarantineStatus::RejectedOversized);
        assert!(item.raw_sha256.is_empty());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn dirty_source_marks_dirty_flag() {
        let path = temp_file("dirty.csv", b"email,phone\na@b.com,123");
        let item = build_intake_item(
            "dirty_public_data",
            SourceClass::DirtyPublicData,
            &path,
            IntakeOptions::default(),
        );
        assert!(item.dirty_flag);
        assert_eq!(item.quarantine_status, QuarantineStatus::Accepted);
        let _ = std::fs::remove_file(path);
    }
}
