//! Bounded audio import and explicit, no-overwrite external-audio bundles.
use base64::{engine::general_purpose::STANDARD, Engine};
use splinely_document::{AudioAsset, Project};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::Path,
};

pub const MAX_ASSET_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_TOTAL_BYTES: usize = 128 * 1024 * 1024;

pub fn asset_bytes(asset: &AudioAsset) -> usize {
    (asset.data_base64.len() / 4 * 3).saturating_sub(
        asset
            .data_base64
            .bytes()
            .rev()
            .take_while(|b| *b == b'=')
            .count()
            .min(2),
    )
}
pub fn total_bytes(project: &Project) -> usize {
    project.audio_assets.iter().map(asset_bytes).sum()
}
fn extension(mime: &str) -> Result<&'static str, String> {
    match mime {
        "audio/wav" | "audio/x-wav" | "audio/wave" => Ok("wav"),
        "audio/mpeg" | "audio/mp3" => Ok("mp3"),
        "audio/ogg" => Ok("ogg"),
        "audio/mp4" | "audio/x-m4a" => Ok("m4a"),
        "audio/flac" | "audio/x-flac" => Ok("flac"),
        "audio/webm" => Ok("webm"),
        _ => Err("Unsupported audio format".into()),
    }
}
pub fn import(path: &str, id: String, project: &Project) -> Result<AudioAsset, String> {
    let path = Path::new(path);
    let mime = match path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "wav" => "audio/wav",
        "mp3" => "audio/mpeg",
        "ogg" | "oga" => "audio/ogg",
        "m4a" | "mp4" => "audio/mp4",
        "flac" => "audio/flac",
        "webm" => "audio/webm",
        _ => return Err("Supported audio: wav, mp3, ogg, m4a, flac, webm".into()),
    };
    if !fs::metadata(path).map_err(|e| e.to_string())?.is_file() {
        return Err("Audio source must be a regular file".into());
    }
    let mut options = OpenOptions::new();
    options.read(true);
    // A source replaced by a FIFO between metadata and open must not block the UI.
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NONBLOCK);
    }
    let file = options.open(path).map_err(|e| e.to_string())?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("Audio source must be a regular file".into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_ASSET_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.is_empty() || bytes.len() > MAX_ASSET_BYTES {
        return Err("Audio import limit: nonempty, at most 32 MiB per asset".into());
    }
    if total_bytes(project).saturating_add(bytes.len()) > MAX_TOTAL_BYTES {
        return Err("Audio import limit: 128 MiB total source bytes".into());
    }
    Ok(AudioAsset {
        id,
        name: path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        mime_type: mime.into(),
        data_base64: STANDARD.encode(bytes),
    })
}
struct BundleDirectory {
    #[cfg(unix)]
    directory: fs::File,
    #[cfg(not(unix))]
    path: std::path::PathBuf,
    /// Windows: handle opened without FILE_SHARE_DELETE, so the directory cannot be
    /// renamed or replaced (e.g. by a junction) while the bundle is written.
    #[cfg(not(unix))]
    _guard: fs::File,
}
#[cfg(windows)]
mod win {
    pub const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    pub const FILE_SHARE_READ: u32 = 0x1;
    pub const FILE_SHARE_WRITE: u32 = 0x2;
    pub const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    pub const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
}
impl BundleDirectory {
    fn reserve(path: &Path) -> Result<Self, String> {
        fs::create_dir(path).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            let directory = OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
                .open(path)
                .map_err(|e| e.to_string())?;
            Ok(Self { directory })
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
            // Open the directory itself (never a reparse-point target) and refuse
            // symlinks/junctions; no FILE_SHARE_DELETE keeps it in place afterwards.
            let guard = OpenOptions::new()
                .read(true)
                .share_mode(win::FILE_SHARE_READ | win::FILE_SHARE_WRITE)
                .custom_flags(win::FILE_FLAG_BACKUP_SEMANTICS | win::FILE_FLAG_OPEN_REPARSE_POINT)
                .open(path)
                .map_err(|e| e.to_string())?;
            let meta = guard.metadata().map_err(|e| e.to_string())?;
            if !meta.is_dir() || meta.file_attributes() & win::FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                return Err("Bundle directory must not be a symlink or junction".into());
            }
            Ok(Self {
                path: path.to_owned(),
                _guard: guard,
            })
        }
        #[cfg(all(not(unix), not(windows)))]
        {
            // Fallback for other targets: refuse symlinks, then create files with create_new.
            let meta = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
            if !meta.is_dir() {
                return Err("Bundle directory must not be a symlink".into());
            }
            Ok(Self {
                path: path.to_owned(),
                _guard: fs::File::open(path).map_err(|e| e.to_string())?,
            })
        }
    }
    fn write(&self, name: &str, data: &[u8]) -> Result<(), String> {
        // Filenames are generated by the exporter, never taken from project data.
        if name.is_empty() || name.contains(['/', '\\']) || name == "." || name == ".." {
            return Err("Invalid bundle filename".into());
        }
        #[cfg(unix)]
        let mut f = {
            use std::os::fd::{AsRawFd, FromRawFd};
            let name = std::ffi::CString::new(name).map_err(|e| e.to_string())?;
            // SAFETY: the directory handle and NUL-terminated name live through openat;
            // a successful descriptor is uniquely owned by the resulting File.
            let fd = unsafe {
                libc::openat(
                    self.directory.as_raw_fd(),
                    name.as_ptr(),
                    libc::O_WRONLY
                        | libc::O_CREAT
                        | libc::O_EXCL
                        | libc::O_NOFOLLOW
                        | libc::O_CLOEXEC,
                    // openat is variadic: the mode must be a c_uint (mode_t is u16 on macOS).
                    0o600 as libc::c_uint,
                )
            };
            if fd < 0 {
                return Err(std::io::Error::last_os_error().to_string());
            }
            unsafe { fs::File::from_raw_fd(fd) }
        };
        #[cfg(not(unix))]
        let mut f = {
            let mut options = OpenOptions::new();
            // create_new fails on any existing entry, including symlinks.
            options.write(true).create_new(true);
            #[cfg(windows)]
            {
                use std::os::windows::fs::OpenOptionsExt;
                options.custom_flags(win::FILE_FLAG_OPEN_REPARSE_POINT);
            }
            options
                .open(self.path.join(name))
                .map_err(|e| e.to_string())?
        };
        f.write_all(data)
            .and_then(|_| f.sync_all())
            .map_err(|e| e.to_string())
    }
}
/// Reserve a new directory exclusively and publish SVG last. Failed output is retained.
/// Neither project data nor untrusted asset names become filesystem paths.
pub fn export_bundle(
    project: &Project,
    destination: &Path,
    trusted_js: bool,
) -> Result<(), String> {
    project.validate().map_err(|e| e.to_string())?;
    if total_bytes(project) > MAX_TOTAL_BYTES {
        return Err("External export limit: 128 MiB total audio".into());
    }
    let mut stripped = project.clone();
    let mut files = Vec::new();
    for (i, asset) in stripped.audio_assets.iter_mut().enumerate() {
        if asset_bytes(asset) > MAX_ASSET_BYTES {
            return Err("External export limit: 32 MiB per asset".into());
        }
        let name = format!("audio-{i}.{}", extension(&asset.mime_type)?);
        let data = STANDARD
            .decode(&asset.data_base64)
            .map_err(|e| e.to_string())?;
        files.push((name, data));
        asset.data_base64.clear();
    }
    // The loader below rewrites the metadata with JSON.parse/stringify, so the bundle keeps the JSON embedding.
    let mut svg = stripped
        .to_svg_with_options_embed(
            crate::RUNTIME,
            splinely_document::BrowserExportOptions {
                trusted_js,
                audio_controls: true,
            },
            splinely_document::EmbedFormat::Json,
        )
        .map_err(|e| e.to_string())?;
    // Keep the standard runtime unchanged: hydrate metadata before inserting its script.
    // Insertion preserves document.currentScript for runtime and browser host bootstrap.
    let start = svg
        .find("<script type=\"application/ecmascript\"><![CDATA[")
        .ok_or("Missing runtime script")?;
    let content = start + "<script type=\"application/ecmascript\"><![CDATA[".len();
    let end = svg[content..]
        .find("]]></script>")
        .ok_or("Missing runtime end")?
        + content;
    let runtime = svg[content..end].replace("]]]]><![CDATA[>", "]]>");
    let names: Vec<_> = files.iter().map(|(name, _)| name).collect();
    let loader = format!(
        r#"<script type="application/ecmascript"><![CDATA[
(async()=>{{
const source=document.currentScript,svg=source.ownerSVGElement,meta=svg.querySelector('[id="splinely-project"]');
try{{
const data=JSON.parse(meta.textContent),names={names};let total=0;
for(let i=0;i<names.length;i++){{
 const response=await fetch(new URL(names[i],document.baseURI),{{credentials:'same-origin'}});
 if(!response.ok)throw new Error('Audio fetch failed: '+names[i]);
 const reader=response.body.getReader();let length=0,raw='';
 try{{for(;;){{const part=await reader.read();if(part.done)break;
  length+=part.value.length;total+=part.value.length;
  if(length>33554432 || total>134217728)throw new Error('Audio exceeds 32 MiB per asset / 128 MiB total');
  for(let j=0;j<part.value.length;j+=8192)raw+=String.fromCharCode(...part.value.subarray(j,j+8192));
 }}}}finally{{await reader.cancel();reader.releaseLock();}}
 data.audioAssets[i].dataBase64=btoa(raw);
}}
meta.textContent=JSON.stringify(data);
const script=document.createElementNS('http://www.w3.org/2000/svg','script');
script.textContent={runtime};svg.appendChild(script);
}}catch(error){{console.error('Splinely external audio: static SVG retained',error);}}
}})();
]]></script>"#,
        names = serde_json::to_string(&names).map_err(|e| e.to_string())?,
        runtime = serde_json::to_string(&runtime)
            .map_err(|e| e.to_string())?
            .replace("]]>", "]]\\u003e")
    );
    svg.replace_range(start..end + "]]></script>".len(), &loader);
    publish(destination, &files, &svg)
}
fn publish(destination: &Path, files: &[(String, Vec<u8>)], svg: &str) -> Result<(), String> {
    let directory = BundleDirectory::reserve(destination)?;
    // Unix writes stay relative to the acquired handle even if the path is replaced.
    // Do not recursively clean up through a potentially replaced pathname.
    for (name, bytes) in files {
        directory.write(name, bytes).map_err(|e| {
            format!(
                "{e}; incomplete bundle retained at {}",
                destination.display()
            )
        })?;
    }
    directory
        .write("animation.svg", svg.as_bytes())
        .map_err(|e| {
            format!(
                "{e}; incomplete bundle retained at {}",
                destination.display()
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn project() -> Project {
        let mut p = Project::default();
        p.audio_assets.push(AudioAsset {
            id: "audio".into(),
            name: "../../escape.wav".into(),
            mime_type: "audio/wav".into(),
            data_base64: STANDARD.encode([1, 2, 3, 4]),
        });
        p
    }
    fn dir() -> std::path::PathBuf {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        std::env::temp_dir().join(format!(
            "splinely-bundle-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ))
    }
    #[test]
    fn external_bundle_is_explicit_safe_and_no_overwrite() {
        let mut p = project();
        p.custom_js = "// </script> ]]> & escaped user code".into();
        let original = p.clone();
        let d = dir();
        export_bundle(&p, &d, true).unwrap();
        assert_eq!(fs::read(d.join("audio-0.wav")).unwrap(), [1, 2, 3, 4]);
        let svg = fs::read_to_string(d.join("animation.svg")).unwrap();
        roxmltree::Document::parse(&svg).unwrap();
        assert!(svg.contains("fetch(new URL"));
        assert!(!svg.contains(&p.audio_assets[0].data_base64));
        assert!(export_bundle(&p, &d, false).is_err());
        assert_eq!(fs::read(d.join("audio-0.wav")).unwrap(), [1, 2, 3, 4]);
        assert_eq!(p, original);
        fs::remove_dir_all(d).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn bundle_writes_do_not_follow_replaced_parent() {
        let destination = dir();
        let moved = destination.with_extension("moved");
        let outside = destination.with_extension("outside");
        fs::create_dir(&outside).unwrap();
        let directory = BundleDirectory::reserve(&destination).unwrap();
        fs::rename(&destination, &moved).unwrap();
        std::os::unix::fs::symlink(&outside, &destination).unwrap();
        directory.write("audio-0.wav", b"safe").unwrap();
        assert!(!outside.join("audio-0.wav").exists());
        assert_eq!(fs::read(moved.join("audio-0.wav")).unwrap(), b"safe");
        assert!(directory.write("../escape", b"bad").is_err());
        fs::remove_file(destination).unwrap();
        fs::remove_dir_all(moved).unwrap();
        fs::remove_dir_all(outside).unwrap();
    }
    #[test]
    fn external_export_preserves_trust_in_fresh_directories() {
        let mut p = project();
        p.custom_js = "// hostile </script> ]]> ".into();
        for trusted in [false, true] {
            let d = dir();
            export_bundle(&p, &d, trusted).unwrap();
            let svg = fs::read_to_string(d.join("animation.svg")).unwrap();
            roxmltree::Document::parse(&svg).unwrap();
            assert_eq!(svg.contains("new Function"), trusted);
            assert!(svg.contains("hostile"));
            fs::remove_dir_all(d).unwrap();
        }
    }
    #[test]
    fn embedded_default_and_exact_sizes() {
        let p = project();
        assert_eq!(total_bytes(&p), 4);
        // The default embedding is binary `.sply` (base64 of the file); the audio survives it.
        let svg = crate::export::interactive(&p, false).unwrap();
        let doc = roxmltree::Document::parse(&svg).unwrap();
        let meta = doc
            .descendants()
            .find(|n| n.attribute("id") == Some("splinely-project"))
            .unwrap();
        assert_eq!(meta.attribute("data-encoding"), Some("sply-base64"));
        let embedded =
            Project::from_bytes(&STANDARD.decode(meta.text().unwrap()).unwrap()).unwrap();
        assert_eq!(embedded.audio_assets[0].data_base64, "AQIDBA==");
        assert!(
            crate::export::interactive_with(&p, false, splinely_document::EmbedFormat::Json)
                .unwrap()
                .contains("AQIDBA==")
        );
        assert_eq!(
            asset_bytes(&AudioAsset {
                data_base64: "".into(),
                ..p.audio_assets[0].clone()
            }),
            0
        );
    }
    #[test]
    fn failed_preflight_leaves_no_output() {
        let mut p = project();
        p.audio_assets[0].mime_type = "audio/unknown".into();
        let d = dir();
        assert!(export_bundle(&p, &d, false).is_err());
        assert!(!d.exists());
    }
    #[test]
    fn failed_write_retains_partial_output_without_overwriting() {
        let d = dir();
        let files = vec![
            ("audio-0.wav".into(), vec![1]),
            ("audio-0.wav".into(), vec![2]),
        ];
        assert!(publish(&d, &files, "svg").is_err());
        assert_eq!(fs::read(d.join("audio-0.wav")).unwrap(), [1]);
        assert!(!d.join("animation.svg").exists());
        assert!(export_bundle(&project(), &d, false).is_err());
        fs::remove_dir_all(d).unwrap();
    }
    #[test]
    fn bounded_import_rejects_empty_oversize_and_unknown_files() {
        let d = dir();
        fs::create_dir(&d).unwrap();
        let path = d.join("sound.wav");
        fs::write(&path, []).unwrap();
        assert!(import(path.to_str().unwrap(), "a".into(), &Project::default()).is_err());
        fs::write(&path, [1, 2]).unwrap();
        let a = import(path.to_str().unwrap(), "a".into(), &Project::default()).unwrap();
        assert_eq!(asset_bytes(&a), 2);
        assert_eq!(a.data_base64, "AQI=");
        fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_len(MAX_ASSET_BYTES as u64 + 1)
            .unwrap();
        assert!(import(path.to_str().unwrap(), "a".into(), &Project::default()).is_err());
        assert!(import(d.to_str().unwrap(), "a".into(), &Project::default()).is_err());
        fs::remove_dir_all(d).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn fifo_import_is_rejected_without_waiting_for_a_writer() {
        let d = dir();
        fs::create_dir(&d).unwrap();
        let p = d.join("pipe.wav");
        use std::os::unix::ffi::OsStrExt;
        let name = std::ffi::CString::new(p.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
        assert!(import(p.to_str().unwrap(), "a".into(), &Project::default()).is_err());
        fs::remove_dir_all(d).unwrap();
    }
    #[test]
    fn existing_file_and_symlink_are_not_overwritten() {
        let d = dir();
        fs::write(&d, b"keep").unwrap();
        assert!(export_bundle(&project(), &d, false).is_err());
        assert_eq!(fs::read(&d).unwrap(), b"keep");
        fs::remove_file(d).unwrap();
        #[cfg(unix)]
        {
            let d = dir();
            std::os::unix::fs::symlink(std::env::temp_dir(), &d).unwrap();
            assert!(export_bundle(&project(), &d, false).is_err());
            fs::remove_file(d).unwrap();
        }
    }
}
