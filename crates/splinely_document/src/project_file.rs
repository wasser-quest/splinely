//! Single-file authored projects: strict UTF-8 JSON, conventionally named `.sply`.
use crate::{DocumentError, Project};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Maximum encoded project file size (64 MiB), including whitespace and newline.
pub const MAX_PROJECT_FILE_BYTES: u64 = 64 * 1024 * 1024;

impl Project {
    /// Load a bounded UTF-8 project with the same strict rules as `from_json`.
    /// Extensions are conventions, not parser switches; legacy JSON is accepted.
    pub fn load_file(path: impl AsRef<Path>) -> Result<Self, DocumentError> {
        let path = path.as_ref();
        let load = || -> Result<Self, DocumentError> {
            let mut source = String::new();
            File::open(path)
                .map_err(io_error)?
                .take(MAX_PROJECT_FILE_BYTES + 1)
                .read_to_string(&mut source)
                .map_err(io_error)?;
            if source.len() as u64 > MAX_PROJECT_FILE_BYTES {
                return Err(DocumentError("Project file exceeds 64 MiB limit".into()));
            }
            Self::from_json(&source)
        };
        load().map_err(|e| DocumentError(format!("{}: {e}", path.display())))
    }

    /// Validate and atomically replace a project file with canonical pretty JSON.
    /// The parent must exist. No code executes or external resources are fetched.
    /// Failed writes leave the destination unchanged; no multi-writer lock or
    /// directory-entry power-loss durability guarantee is provided.
    pub fn save_file(&self, path: impl AsRef<Path>) -> Result<(), DocumentError> {
        self.validate()?;
        let path = path.as_ref();
        let save = || -> Result<(), DocumentError> {
            let parent = path
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new("."));
            let (mut temporary, file) = Temporary::create(parent).map_err(io_error)?;
            let mut writer = BoundedWriter { file, written: 0 };
            serde_json::to_writer_pretty(&mut writer, self)?;
            writer.write_all(b"\n").map_err(io_error)?;
            writer.file.sync_all().map_err(io_error)?;
            drop(writer); // Close before rename, including on Windows.
            fs::rename(temporary.0.as_ref().expect("temporary path"), path).map_err(io_error)?;
            temporary.0 = None;
            Ok(())
        };
        save().map_err(|e| DocumentError(format!("{}: {e}", path.display())))
    }
}

fn io_error(error: io::Error) -> DocumentError {
    DocumentError(error.to_string())
}

struct BoundedWriter {
    file: File,
    written: u64,
}
impl Write for BoundedWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() as u64 > MAX_PROJECT_FILE_BYTES - self.written {
            return Err(io::Error::other("Project file exceeds 64 MiB limit"));
        }
        let count = self.file.write(bytes)?;
        self.written += count as u64;
        Ok(count)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

struct Temporary(Option<PathBuf>);
impl Temporary {
    fn create(parent: &Path) -> io::Result<(Self, File)> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        for _ in 0..128 {
            let id = NEXT.fetch_add(1, Ordering::Relaxed);
            let path = parent.join(format!(".splinely-{}-{id}.tmp", std::process::id()));
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            match options.open(&path) {
                Ok(file) => return Ok((Self(Some(path)), file)),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "Unable to create unique project temporary file",
        ))
    }
}
impl Drop for Temporary {
    fn drop(&mut self) {
        if let Some(path) = &self.0 {
            let _ = fs::remove_file(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn directory() -> Temporary {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "splinely-file-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Temporary(Some(path))
    }

    #[test]
    fn strict_roundtrip_and_atomic_failures() {
        let mut dir = directory();
        let root = dir.0.as_ref().unwrap();
        let path = root.join("project.sply");
        let mut project = Project {
            custom_js: "throw new Error('never run');".into(),
            raw_svg: "<g/>".into(),
            ..Project::default()
        };
        project.save_file(&path).unwrap();
        assert_eq!(
            Project::load_file(&path).unwrap().to_json().unwrap(),
            project.to_json().unwrap()
        );
        let original = fs::read(&path).unwrap();
        let blocked = root.join("directory.sply");
        fs::create_dir(&blocked).unwrap();
        assert!(project.save_file(&blocked).is_err());
        assert!(blocked.is_dir());
        fs::remove_dir(&blocked).unwrap();
        assert_eq!(fs::read_dir(root).unwrap().count(), 1);
        assert!(project.save_file(root.join("missing/project.sply")).is_err());
        project.width = -1.0;
        assert!(project.save_file(&path).is_err());
        assert_eq!(fs::read(&path).unwrap(), original);
        project.width = 640.0;
        project.custom_js = "x".repeat(MAX_PROJECT_FILE_BYTES as usize);
        assert!(project.save_file(&path).is_err());
        assert_eq!(fs::read(&path).unwrap(), original);
        assert_eq!(fs::read_dir(root).unwrap().count(), 1);
        for source in [
            r#"{"name":"a","name":"b"}"#,
            r#"{"unknown":true}"#,
            r#"{"width":18446744073709551616}"#,
            r#"{"width":9007199254740993}"#,
        ] {
            fs::write(&path, source).unwrap();
            assert!(Project::load_file(&path).is_err());
        }
        File::create(&path)
            .unwrap()
            .set_len(MAX_PROJECT_FILE_BYTES + 1)
            .unwrap();
        assert!(Project::load_file(&path).is_err());
        fs::remove_dir_all(root).unwrap();
        dir.0 = None;
    }
}
