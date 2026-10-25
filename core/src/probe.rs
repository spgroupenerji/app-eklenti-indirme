use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tur {
    Http,
    Hls,
    Dash,
}

#[derive(Debug)]
pub struct HedefBilgisi {
    pub son_url: String,
    pub boyut: Option<u64>,
    pub aralik_destekli: bool,
    pub icerik_turu: Option<String>,
    pub dosya_adi: Option<String>,
    pub tur: Tur,
}

pub fn basliklara_cevir(headers: &HashMap<String, String>) -> HeaderMap {
    let mut harita = HeaderMap::new();
    for (k, v) in headers {
        if let (Ok(ad), Ok(deger)) = (
            HeaderName::try_from(k.as_str()),
            HeaderValue::from_str(v),
        ) {
            harita.insert(ad, deger);
        }
    }
    harita
}

fn sunucu_yalaniciligi(status: reqwest::StatusCode) -> bool {
    status == reqwest::StatusCode::METHOD_NOT_ALLOWED
        || status == reqwest::StatusCode::NOT_IMPLEMENTED
        || status == reqwest::StatusCode::BAD_REQUEST
}

pub async fn kesfet(
    client: &reqwest::Client,
    url: &str,
    headers: &HashMap<String, String>,
) -> Result<HedefBilgisi, String> {
    let base = basliklara_cevir(headers);

    let head = client
        .head(url)
        .headers(base.clone())
        .send()
        .await
        .map_err(|e| format!("Sunucuya ulaşılamadı: {e}"))?;

    if !sunucu_yalaniciligi(head.status()) && head.status().is_success() {
        let h = head.headers();
        let boyut = h
            .get(reqwest::header::CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok());
        let aralik = h
            .get(reqwest::header::ACCEPT_RANGES)
            .and_then(|v| v.to_str().ok())
            .map(|v| v.eq_ignore_ascii_case("bytes"))
            .unwrap_or(false);
        let tur_str = h
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|v| v.to_string());
        let ad = h
            .get(reqwest::header::CONTENT_DISPOSITION)
            .and_then(|v| v.to_str().ok())
            .and_then(crate::util::filename_from_disposition);
        let son_url = head.url().to_string();
        return Ok(siniflandir(son_url, boyut, aralik, tur_str, ad));
    }

    // HEAD reddedildi: tek baytlık Range ile keşif
    let yanit = client
        .get(url)
        .headers(base.clone())
        .header("Range", "bytes=0-0")
        .send()
        .await
        .map_err(|e| format!("Sunucuya ulaşılamadı: {e}"))?;
    let durum = yanit.status();
    if !durum.is_success() {
        return Err(format!("Sunucu {durum} yanıtı verdi"));
    }
    let h = yanit.headers();
    let (boyut, aralik) = if durum == reqwest::StatusCode::PARTIAL_CONTENT {
        let toplam = h
            .get(reqwest::header::CONTENT_RANGE)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.rsplit('/').next())
            .and_then(|v| v.parse::<u64>().ok());
        (toplam, true)
    } else {
        let toplam = h
            .get(reqwest::header::CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok());
        let aralik = h
            .get(reqwest::header::ACCEPT_RANGES)
            .and_then(|v| v.to_str().ok())
            .map(|v| v.eq_ignore_ascii_case("bytes"))
            .unwrap_or(false);
        (toplam, aralik)
    };
    let tur_str = h
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.to_string());
    let ad = h
        .get(reqwest::header::CONTENT_DISPOSITION)
        .and_then(|v| v.to_str().ok())
        .and_then(crate::util::filename_from_disposition);
    let son_url = yanit.url().to_string();
    Ok(siniflandir(son_url, boyut, aralik, tur_str, ad))
}

fn siniflandir(
    son_url: String,
    boyut: Option<u64>,
    aralik: bool,
    icerik_turu: Option<String>,
    dosya_adi: Option<String>,
) -> HedefBilgisi {
    let tur_kucuk = icerik_turu
        .as_deref()
        .unwrap_or("")
        .to_ascii_lowercase();
    let yol_kucuk = son_url.to_ascii_lowercase();
    let tur = if tur_kucuk.contains("mpegurl")
        || tur_kucuk.contains("x-mpegurl")
        || yol_kucuk.contains(".m3u8")
    {
        Tur::Hls
    } else if tur_kucuk.contains("dash+xml") || yol_kucuk.contains(".mpd") {
        Tur::Dash
    } else {
        Tur::Http
    };
    HedefBilgisi {
        son_url,
        boyut,
        aralik_destekli: aralik,
        icerik_turu: icerik_turu,
        dosya_adi,
        tur,
    }
}
