// Sabitler ve yardımcılar
pub const MIN_SPLIT_BYTES: u64 = 4 * 1024 * 1024;
pub const WINDOW_BYTES: u64 = 1024 * 1024;
pub const STEAL_THRESHOLD: u64 = 2 * MIN_SPLIT_BYTES;
pub const MAX_RETRIES: u32 = 10;
pub const RETRY_BASE_MS: u64 = 400;
pub const RETRY_MAX_MS: u64 = 8000;
pub const STALL_TIMEOUT_SECS: u64 = 60;
pub const HLS_CONCURRENCY: usize = 8;
pub const MAX_DASH_SEGMENTS: u64 = 100_000;
pub const PROGRESS_INTERVAL_MS: u64 = 500;
pub const REFRESH_TIMEOUT_SECS: u64 = 600;
pub const STATE_EXT: &str = "titan_state";

pub fn sanitize_filename(raw: &str) -> String {
    let mut out: String = raw
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '-',
            c if (c as u32) < 0x20 => '-',
            c => c,
        })
        .collect();
    while out.len() > 180 {
        out.pop();
    }
    while out.ends_with('.') || out.ends_with(' ') {
        out.pop();
    }
    if out.is_empty() {
        out = "indirme".into();
    }
    const RESERVED: [&str; 22] = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7",
        "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    let stem = out.split('.').next().unwrap_or("").to_uppercase();
    if RESERVED.contains(&stem.as_str()) {
        format!("_{out}")
    } else {
        out
    }
}

pub fn unique_path(dir: &std::path::Path, name: &str) -> std::path::PathBuf {
    let mut aday = dir.join(name);
    if !aday.exists() {
        return aday;
    }
    let (stem, ext) = match name.rfind('.') {
        Some(i) => (&name[..i], &name[i..]),
        None => (name, ""),
    };
    for i in 1..u32::MAX {
        aday = dir.join(format!("{stem} ({i}){ext}"));
        if !aday.exists() {
            return aday;
        }
    }
    aday
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = |b: u8| (b as char).to_digit(16);
            if let (Some(h), Some(l)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub fn filename_from_disposition(header: &str) -> Option<String> {
    let mut plain = None;
    let mut extended = None;
    for part in header.split(';') {
        let part = part.trim();
        if let Some(v) = part.strip_prefix("filename*=").or(part.strip_prefix("FILENAME*=")) {
            let v = v.trim_matches('"');
            if let Some((charset, rest)) = v.split_once('\'') {
                if let Some((_, raw)) = rest.split_once('\'') {
                    let decoded = match charset.to_ascii_lowercase().as_str() {
                        "utf-8" => percent_decode(raw),
                        _ => raw.to_string(),
                    };
                    extended = Some(decoded);
                }
            }
        } else if let Some(v) = part.strip_prefix("filename=").or(part.strip_prefix("FILENAME=")) {
            plain = Some(v.trim_matches('"').to_string());
        }
    }
    extended.or(plain)
}

pub fn filename_from_url(url: &str) -> Option<String> {
    let yol = url::Url::parse(url).ok()?;
    let son = yol.path_segments()?.last()?;
    let cozulmus = percent_decode(son);
    let temiz = sanitize_filename(&cozulmus);
    if temiz.is_empty() { None } else { Some(temiz) }
}

pub fn job_id(url: &str, filename: &str) -> String {
    blake3::hash(format!("{url}\u{0}{filename}").as_bytes())
        .to_hex()
        .as_str()[..16]
        .to_string()
}

pub fn iso8601_duration_to_secs(s: &str) -> Option<f64> {
    let govde = s.strip_prefix('P')?;
    let (gun_kismi, saat_kismi) = govde.split_once('T').unwrap_or((govde, ""));
    let mut toplam = 0.0f64;
    let mut sayi = String::new();
    for c in gun_kismi.chars().chain(std::iter::once('D')) {
        if c.is_ascii_digit() || c == '.' {
            sayi.push(c);
        } else {
            if sayi.is_empty() {
                continue;
            }
            let v: f64 = sayi.parse().ok()?;
            toplam += match c {
                'D' => v * 86400.0,
                'Y' => v * 31_557_600.0,
                'M' => v * 2_629_800.0,
                'W' => v * 604_800.0,
                _ => 0.0,
            };
            sayi.clear();
        }
    }
    let mut sayi = String::new();
    for c in saat_kismi.chars().chain(std::iter::once('X')) {
        if c.is_ascii_digit() || c == '.' {
            sayi.push(c);
        } else {
            if sayi.is_empty() {
                continue;
            }
            let v: f64 = sayi.parse().ok()?;
            toplam += match c {
                'H' => v * 3600.0,
                'M' => v * 60.0,
                'S' => v,
                _ => 0.0,
            };
            sayi.clear();
        }
    }
    if toplam > 0.0 { Some(toplam) } else { None }
}
