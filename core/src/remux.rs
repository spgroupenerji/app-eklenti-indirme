use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::io::AsyncWriteExt;
use tokio::process::{Child, ChildStdin, Command};

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

static FFMPEG_YOL: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();

/// FFmpeg şu sırayla aranır: exe'nin yanındaki ffmpeg.exe, sonra PATH.
pub fn ffmpeg_yolu() -> Option<&'static Path> {
    FFMPEG_YOL
        .get_or_init(|| {
            let yanindaki = std::env::current_exe()
                .ok()
                .and_then(|e| e.parent().map(|p| p.join("ffmpeg.exe")))
                .filter(|p| p.is_file());
            let aday = yanindaki.unwrap_or_else(|| PathBuf::from("ffmpeg"));
            let mut komut = std::process::Command::new(&aday);
            komut.arg("-version").stdout(Stdio::null()).stderr(Stdio::null());
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                komut.creation_flags(CREATE_NO_WINDOW);
            }
            komut.status().is_ok_and(|s| s.success()).then_some(aday)
        })
        .as_deref()
}

pub fn ffmpeg_var() -> bool {
    ffmpeg_yolu().is_some()
}

pub fn url_birlestir(taban: &str, bagil: &str) -> Option<String> {
    if bagil.starts_with("http://") || bagil.starts_with("https://") {
        return Some(bagil.to_string());
    }
    let taban_url = url::Url::parse(taban).ok()?;
    taban_url.join(bagil).ok().map(|u| u.to_string())
}

pub struct Remux {
    cocuk: Child,
    stdin: Option<ChildStdin>,
    hata_yolu: PathBuf,
}

impl Remux {
    pub fn boru_baslat(cikis: &Path) -> Option<Self> {
        if let Some(ust) = cikis.parent() {
            let _ = std::fs::create_dir_all(ust);
        }
        let ffmpeg = ffmpeg_yolu()?;
        let mut komut = Command::new(ffmpeg);
        komut
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-i",
                "pipe:0",
                "-c",
                "copy",
                "-movflags",
                "+faststart",
            ])
            .arg(cikis)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        komut.creation_flags(CREATE_NO_WINDOW);
        let mut cocuk = komut.spawn().ok()?;
        let stdin = cocuk.stdin.take()?;
        Some(Self {
            cocuk,
            stdin: Some(stdin),
            hata_yolu: cikis.to_path_buf(),
        })
    }

    pub async fn yaz(&mut self, veri: &[u8]) -> Result<(), String> {
        let Some(stdin) = self.stdin.as_mut() else {
            return Err("ffmpeg stdin zaten kapalı".into());
        };
        stdin
            .write_all(veri)
            .await
            .map_err(|_| format!("ffmpeg erken kapandı: {}", self.hata_yolu.display()))
    }

    pub async fn bekle(mut self) -> Result<(), String> {
        drop(self.stdin.take());
        let cikis = self
            .cocuk
            .wait_with_output()
            .await
            .map_err(|e| format!("ffmpeg beklenemedi: {e}"))?;
        if cikis.status.success() {
            Ok(())
        } else {
            let log = String::from_utf8_lossy(&cikis.stderr);
            Err(format!(
                "ffmpeg remux başarısız ({}): {}",
                cikis.status,
                log.trim()
            ))
        }
    }
}

pub async fn mux_iki(video: &Path, ses: Option<&Path>, cikis: &Path) -> Result<(), String> {
    let ffmpeg = ffmpeg_yolu().ok_or("ffmpeg bulunamadı")?;
    let mut komut = Command::new(ffmpeg);
    komut
        .args(["-hide_banner", "-loglevel", "error", "-y"])
        .arg("-i")
        .arg(video);
    if let Some(s) = ses {
        komut.arg("-i").arg(s);
    }
    komut
        .args(["-c", "copy", "-movflags", "+faststart"])
        .arg(cikis)
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    komut.creation_flags(CREATE_NO_WINDOW);
    let cikis = komut
        .output()
        .await
        .map_err(|e| format!("ffmpeg başlatılamadı: {e}"))?;
    if cikis.status.success() {
        Ok(())
    } else {
        Err(format!(
            "ffmpeg birleştirme başarısız: {}",
            String::from_utf8_lossy(&cikis.stderr).trim()
        ))
    }
}
