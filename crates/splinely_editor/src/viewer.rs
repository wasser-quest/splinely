//! Launch the sibling native WebView viewer with an immutable SVG snapshot.
use std::{
    fs,
    path::PathBuf,
    process::{Child, Command},
    time::{SystemTime, UNIX_EPOCH},
};

pub struct ViewerSession {
    child: Child,
    directory: PathBuf,
}
impl ViewerSession {
    pub fn open(svg: &str) -> Result<Self, String> {
        let executable = std::env::current_exe().map_err(|e| e.to_string())?;
        let viewer = executable
            .parent()
            .ok_or("Editor executable has no parent")?
            .join(format!("splinely-viewer{}", std::env::consts::EXE_SUFFIX));
        if !viewer.is_file() {
            return Err("Splinely Viewer fehlt. Zuerst cargo build --workspace ausführen (Linux: GTK3/WebKitGTK benötigt).".into());
        }
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let directory =
            std::env::temp_dir().join(format!("splinely-viewer-{}-{stamp}", std::process::id()));
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(&directory).map_err(|e| e.to_string())?;
        let path = directory.join("scene.svg");
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let saved = options.open(&path).and_then(|mut file| {
            use std::io::Write;
            file.write_all(svg.as_bytes())
        });
        if let Err(error) = saved {
            let _ = fs::remove_dir_all(&directory);
            return Err(error.to_string());
        }
        let child = match Command::new(viewer).arg(&path).spawn() {
            Ok(child) => child,
            Err(error) => {
                let _ = fs::remove_dir_all(&directory);
                return Err(format!("Viewer konnte nicht starten: {error}"));
            }
        };
        Ok(Self { child, directory })
    }
    pub fn finished(&mut self) -> Result<bool, String> {
        match self.child.try_wait().map_err(|e| e.to_string())? {
            Some(status) if !status.success() => Err(format!("Viewer beendet: {status}")),
            Some(_) => Ok(true),
            None => Ok(false),
        }
    }
}
impl Drop for ViewerSession {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_dir_all(&self.directory);
    }
}
