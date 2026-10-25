use crate::engine::{http_cek, IsKontrol, IlerlemeRef};
use crate::protocol::Event;
use crate::remux::{ffmpeg_var, url_birlestir, Remux};
use crate::util::{sanitize_filename, HLS_CONCURRENCY, MAX_RETRIES};
use futures_util::stream::{self, StreamExt};
use tokio::io::AsyncWriteExt;
use m3u8_rs::{MasterPlaylist, MediaPlaylist};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;

enum AnahtarDurumu {
    Yok,
    Aes128 { uri: String, iv_hex: Option<String> },
}

struct Harita {
    url: String,
    aralik: Option<(u64, u64)>,
}

struct Parca {
    url: String,
    aralik: Option<(u64, u64)>,
    anahtar: AnahtarDurumu,
    mutlak_sira: u64,
}

fn cozumle(metin: &str) -> Result<m3u8_rs::Playlist, String> {
    m3u8_rs::parse_playlist_res(metin.as_bytes())
        .map_err(|e| format!("m3u8 ayrıştırılamadı: {e}"))
}

fn en_iyi_varyant(ana: &MasterPlaylist, taban: &str) -> Result<String, String> {
    let en_iyi = ana
        .variants
        .iter()
        .max_by(|a, b| {
            let puan = |v: &m3u8_rs::VariantStream| {
                let alan = v
                    .resolution
                    .as_ref()
                    .map_or(0u64, |r| (r.width as u64) * (r.height as u64));
                (v.bandwidth as u128, alan)
            };
            puan(a).cmp(&puan(b))
        })
        .ok_or("Ana oynatma listesinde varyant yok")?;
    url_birlestir(taban, &en_iyi.uri).ok_or_else(|| "Varyant adresi çözülemedi".into())
}

enum KayitYazici {
    Dosya(tokio::fs::File),
    Boru(Remux),
}

impl KayitYazici {
    async fn yaz(&mut self, veri: &[u8]) -> Result<(), String> {
        match self {
            KayitYazici::Dosya(f) => f.write_all(veri).await.map_err(|e| e.to_string()),
            KayitYazici::Boru(r) => r.yaz(veri).await,
        }
    }
    async fn kapat(self) -> Result<(), String> {
        match self {
            KayitYazici::Dosya(f) => f.sync_data().await.map_err(|e| e.to_string()),
            KayitYazici::Boru(r) => r.bekle().await,
        }
    }
}

pub async fn hls_indir(
    client: reqwest::Client,
    kontrol: Arc<IsKontrol>,
    ilerleme: IlerlemeRef,
    olay: tokio::sync::mpsc::UnboundedSender<Event>,
    dizin: &std::path::Path,
    istenen_ad: &str,
) -> Result<PathBuf, String> {
    let (url, _) = kontrol.baglam_oku();
    let metin = String::from_utf8_lossy(&http_cek(&client, &kontrol, &olay, &url, None, 5).await?)
        .into_owned();

    let (medya_url, liste): (String, MediaPlaylist) = match cozumle(&metin)? {
        m3u8_rs::Playlist::MasterPlaylist(ana) => {
            let varyant = en_iyi_varyant(&ana, &url)?;
            let metin2 = String::from_utf8_lossy(
                &http_cek(&client, &kontrol, &olay, &varyant, None, 5).await?,
            )
            .into_owned();
            match cozumle(&metin2)? {
                m3u8_rs::Playlist::MediaPlaylist(m) => (varyant, m),
                _ => return Err("Varyant liste zinciri geçersiz".into()),
            }
        }
        m3u8_rs::Playlist::MediaPlaylist(m) => (url, m),
    };

    if !liste.end_list {
        return Err("Bu bir canlı yayın akışı; yalnızca sabit (VOD) akışlar destekleniyor".into());
    }
    if liste.segments.is_empty() {
        return Err("Oynatma listesinde parça yok".into());
    }

    let mut anahtar = AnahtarDurumu::Yok;
    let mut anahtar_urileri: Vec<String> = Vec::new();
    let mut harita: Option<Harita> = None;
    let mut parcalar: Vec<Parca> = Vec::with_capacity(liste.segments.len());
    let mut bayt_imleci: Option<u64> = None;

    if let Some(m) = liste.segments.iter().find_map(|s| s.map.as_ref()) {
        let adres = url_birlestir(&medya_url, &m.uri).ok_or("EXT-X-MAP adresi çözülemedi")?;
        let aralik = m.byte_range.as_ref().map(|br| {
            let bas = br.offset.unwrap_or(0);
            (bas, bas + br.length.saturating_sub(1))
        });
        harita = Some(Harita { url: adres, aralik });
    }

    for (i, seg) in liste.segments.iter().enumerate() {
        if let Some(k) = &seg.key {
            let keyformat_tanimli = k.keyformat.is_some();
            anahtar = match k.method {
                m3u8_rs::KeyMethod::None => AnahtarDurumu::Yok,
                m3u8_rs::KeyMethod::SampleAES => {
                    return Err("Akış SAMPLE-AES ile korunuyor; DRM korumalı akışlar indirilemez".into())
                }
                m3u8_rs::KeyMethod::AES128 => {
                    let uri = k.uri.clone().ok_or("AES-128 anahtarı URI'siz")?;
                    let uri = url_birlestir(&medya_url, &uri).ok_or("Anahtar adresi çözülemedi")?;
                    if !anahtar_urileri.contains(&uri) {
                        anahtar_urileri.push(uri.clone());
                    }
                    AnahtarDurumu::Aes128 { uri, iv_hex: k.iv.clone() }
                }
                m3u8_rs::KeyMethod::Other(ref f) => {
                    if keyformat_tanimli && k.keyformat.as_deref() != Some("identity") {
                        return Err(format!(
                            "Desteklenmeyen anahtar biçimi ({f}); DRM korumalı akışlar indirilemez"
                        ));
                    }
                    let uri = k.uri.clone().ok_or("AES-128 anahtarı URI'siz")?;
                    let uri = url_birlestir(&medya_url, &uri).ok_or("Anahtar adresi çözülemedi")?;
                    if !anahtar_urileri.contains(&uri) {
                        anahtar_urileri.push(uri.clone());
                    }
                    AnahtarDurumu::Aes128 { uri, iv_hex: k.iv.clone() }
                }
            };
        }
        let parca_url = url_birlestir(&medya_url, &seg.uri).ok_or("Parça adresi çözülemedi")?;
        let aralik = seg.byte_range.as_ref().map(|br| {
            let bas = br.offset.unwrap_or_else(|| bayt_imleci.unwrap_or(0));
            bayt_imleci = Some(bas + br.length);
            (bas, bas + br.length.saturating_sub(1))
        });
        let anahtar_klon = match &anahtar {
            AnahtarDurumu::Yok => AnahtarDurumu::Yok,
            AnahtarDurumu::Aes128 { uri, iv_hex } => AnahtarDurumu::Aes128 {
                uri: uri.clone(),
                iv_hex: iv_hex.clone(),
            },
        };
        parcalar.push(Parca {
            url: parca_url,
            aralik,
            anahtar: anahtar_klon,
            mutlak_sira: liste.media_sequence + i as u64,
        });
    }

    let mut anahtarlar: HashMap<String, [u8; 16]> = HashMap::new();
    for uri in &anahtar_urileri {
        let veri = http_cek(&client, &kontrol, &olay, uri, None, 5).await?;
        let dizi: [u8; 16] = veri.as_slice().try_into().map_err(|_| "Anahtar 16 bayt olmalı")?;
        anahtarlar.insert(uri.clone(), dizi);
    }

    let fmp4 = harita.is_some();
    let taban = sanitize_filename(istenen_ad.trim_end_matches(".m3u8"));
    let cikis_yolu = dizin.join(format!("{taban}.mp4"));
    let ham_yol = dizin.join(format!("{taban}.{}", if fmp4 { "mp4" } else { "ts" }));

    ilerleme.toplam.store(0, Ordering::SeqCst);

    let harita_veri = match &harita {
        Some(h) => Some(http_cek(&client, &kontrol, &olay, &h.url, h.aralik, MAX_RETRIES).await?),
        None => None,
    };

    let boru = if ffmpeg_var() {
        Remux::boru_baslat(&cikis_yolu)
    } else {
        None
    };
    let toplam_parca = parcalar.len() as u64 + u64::from(harita_veri.is_some());
    let mut islenen: u64 = 0;

    let boru_kullandi = boru.is_some();
    let mut kayit = match boru {
        Some(r) => KayitYazici::Boru(r),
        None => {
            let f = tokio::fs::File::create(&ham_yol)
                .await
                .map_err(|e| format!("Çıktı dosyası açılamadı: {e}"))?;
            KayitYazici::Dosya(f)
        }
    };

    if let Some(v) = &harita_veri {
        kayit.yaz(v).await?;
        islenen += 1;
        ilerleme.detay_yaz(format!("{islenen}/{toplam_parca} parça"));
    }

    let mut akis = stream::iter(parcalar)
        .map(|p| {
            let client = client.clone();
            let kontrol = kontrol.clone();
            let olay = olay.clone();
            let anahtarlar = anahtarlar.clone();
            async move { parca_getir(client, kontrol, olay, anahtarlar, p).await }
        })
        .buffered(HLS_CONCURRENCY);
    while let Some(sonuc) = akis.next().await {
        kontrol.kontrol_noktasi().await?;
        let veri = sonuc?;
        kayit.yaz(&veri).await?;
        islenen += 1;
        ilerleme.indirilen.fetch_add(veri.len() as u64, Ordering::Relaxed);
        ilerleme.detay_yaz(format!("{islenen}/{toplam_parca} parça"));
    }
    kayit.kapat().await?;

    Ok(if boru_kullandi { cikis_yolu } else { ham_yol })
}

async fn parca_getir(
    client: reqwest::Client,
    kontrol: Arc<IsKontrol>,
    olay: tokio::sync::mpsc::UnboundedSender<Event>,
    anahtarlar: HashMap<String, [u8; 16]>,
    p: Parca,
) -> Result<Vec<u8>, String> {
    let veri = http_cek(&client, &kontrol, &olay, &p.url, p.aralik, MAX_RETRIES).await?;
    match &p.anahtar {
        AnahtarDurumu::Yok => Ok(veri),
        AnahtarDurumu::Aes128 { uri, iv_hex } => {
            let anahtar = anahtarlar.get(uri).ok_or("Anahtar önbellekte bulunamadı")?;
            let iv = match iv_hex {
                Some(hex) => hex_to_16(hex)?,
                None => (p.mutlak_sira as u128).to_be_bytes(),
            };
            aes128_coz(&veri, anahtar, &iv)
        }
    }
}

fn hex_to_16(hex: &str) -> Result<[u8; 16], String> {
    let temiz = hex.trim().trim_start_matches("0x").trim_start_matches('X');
    if temiz.len() != 32 || !temiz.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("IV değeri 32 haneli onaltılık olmalı".into());
    }
    let mut dizi = [0u8; 16];
    for i in 0..16 {
        dizi[i] = u8::from_str_radix(&temiz[i * 2..i * 2 + 2], 16)
            .map_err(|_| "IV onaltılık dönüşümü başarısız".to_string())?;
    }
    Ok(dizi)
}

fn aes128_coz(sifreli: &[u8], anahtar: &[u8; 16], iv: &[u8; 16]) -> Result<Vec<u8>, String> {
    use cbc::cipher::{block_padding::Pkcs7, BlockModeDecrypt, KeyIvInit};
    type Cozucu = cbc::Decryptor<aes::Aes128>;
    let cozucu = Cozucu::new_from_slices(anahtar, iv)
        .map_err(|_| "Anahtar/IV uzunluğu hatalı".to_string())?;
    let mut tampon = sifreli.to_vec();
    let acik = cozucu
        .decrypt_padded::<Pkcs7>(&mut tampon)
        .map_err(|_| "AES-128 çözme başarısız; veri bozuk olabilir".to_string())?;
    Ok(acik.to_vec())
}
