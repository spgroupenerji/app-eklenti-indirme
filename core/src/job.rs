use crate::engine::{paralel_indir, tek_akis_indir, IsKontrol, Ilerleme, IlerlemeRef, Bitis};
use crate::protocol::{Command, Event, JobSnapshot};
use crate::probe::{self, Tur};
use crate::storage::{HedefDosya, Jurnal};
use crate::util::*;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

pub struct Kayit {
    pub kontrol: Arc<IsKontrol>,
    pub ilerleme: IlerlemeRef,
    pub durum: Mutex<&'static str>,
    pub dosya_adi: Mutex<String>,
    pub url: Mutex<String>,
}

pub struct Uygulama {
    kayitlar: Mutex<HashMap<String, Arc<Kayit>>>,
    olay: mpsc::UnboundedSender<Event>,
}

fn istemci_kur(eslik: usize) -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .tcp_nodelay(true)
        .connect_timeout(std::time::Duration::from_secs(15))
        .read_timeout(std::time::Duration::from_secs(30))
        .pool_idle_timeout(std::time::Duration::from_secs(90))
        .pool_max_idle_per_host(eslik.max(4))
        .build()
        .map_err(|e| format!("HTTP istemcisi oluşturulamadı: {e}"))
}

fn uzanti_turu(icerik: Option<&str>) -> &'static str {
    let t = icerik.unwrap_or("");
    if t.starts_with("video/mp4") || t.contains("mp4") {
        ".mp4"
    } else if t.starts_with("video/webm") {
        ".webm"
    } else if t.starts_with("video/x-matroska") {
        ".mkv"
    } else if t.starts_with("video/mpeg") || t.contains("mpegurl") {
        ".ts"
    } else if t.starts_with("audio/mpeg") {
        ".mp3"
    } else if t.starts_with("audio/mp4") {
        ".m4a"
    } else if t.starts_with("audio/ogg") || t.starts_with("video/ogg") {
        ".ogg"
    } else if t.starts_with("audio/wav") {
        ".wav"
    } else if t.starts_with("audio/flac") {
        ".flac"
    } else if t.starts_with("image/jpeg") {
        ".jpg"
    } else if t.starts_with("image/png") {
        ".png"
    } else if t.starts_with("image/webp") {
        ".webp"
    } else if t.contains("zip") {
        ".zip"
    } else if t.contains("gzip") {
        ".gz"
    } else if t.contains("pdf") {
        ".pdf"
    } else if t.contains("json") {
        ".json"
    } else {
        ".bin"
    }
}

impl Uygulama {
    pub fn yeni(olay: mpsc::UnboundedSender<Event>) -> Arc<Self> {
        Arc::new(Self {
            kayitlar: Mutex::new(HashMap::new()),
            olay,
        })
    }

    pub fn baslat(
        &self,
        url: String,
        filename: Option<String>,
        headers: HashMap<String, String>,
        concurrency: Option<u32>,
    ) -> String {
        let eslik = concurrency.unwrap_or(16).clamp(1, 32) as usize;
        let aday_ad = filename.unwrap_or_default();
        let id = job_id(&url, &aday_ad);
        {
            let kayitlar = self.kayitlar.lock().unwrap();
            if let Some(mevcut) = kayitlar.get(&id) {
                if *mevcut.durum.lock().unwrap() == "indiriliyor"
                    || *mevcut.durum.lock().unwrap() == "duraklatildi"
                {
                    return id;
                }
            }
        }
        let kontrol = IsKontrol::yeni(id.clone(), url.clone(), headers);
        let ilerleme = Ilerleme::yeni(0, 0);
        let kayit = Arc::new(Kayit {
            kontrol: kontrol.clone(),
            ilerleme: ilerleme.clone(),
            durum: Mutex::new("indiriliyor"),
            dosya_adi: Mutex::new(if aday_ad.is_empty() { "belirleniyor…".into() } else { aday_ad.clone() }),
            url: Mutex::new(url.clone()),
        });
        self.kayitlar.lock().unwrap().insert(id.clone(), kayit.clone());

        let olay = self.olay.clone();
        let id_is = id.clone();
        let kayit_iz = kayit.clone();
        let id_iz = id.clone();
        tokio::spawn(async move {
            let sonuc = gorev_yurut(kayit.clone(), kontrol, ilerleme, olay.clone(), eslik).await;
            match sonuc {
                Ok(yol) => {
                    *kayit.durum.lock().unwrap() = "tamamlandi";
                    let _ = olay.send(Event::completed(&id_is, &yol.to_string_lossy()));
                }
                Err(e) if e == "İPTAL" => {
                    *kayit.durum.lock().unwrap() = "iptal";
                    let _ = olay.send(Event::canceled(&id_is));
                }
                Err(e) => {
                    *kayit.durum.lock().unwrap() = "hata";
                    let _ = olay.send(Event::failed(&id_is, &e));
                }
            }
        });

        // İlerleme izleyici: 500 ms'de bir hız/ETA ile PROGRESS yayınlar
        let olay_iz = self.olay.clone();
        tokio::spawn(async move {
            let mut son_deger = 0u64;
            let mut son_an = tokio::time::Instant::now();
            let mut ema = 0.0f64;
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(PROGRESS_INTERVAL_MS)).await;
                let durum = *kayit_iz.durum.lock().unwrap();
                if durum == "duraklatildi" {
                    continue;
                }
                if durum != "indiriliyor" {
                    return;
                }
                let simdi = tokio::time::Instant::now();
                let deger = kayit_iz.ilerleme.indirilen.load(Ordering::Relaxed);
                let gecen = simdi.duration_since(son_an).as_secs_f64().max(0.001);
                let anlik = (deger.saturating_sub(son_deger)) as f64 / gecen;
                ema = if ema == 0.0 { anlik } else { ema * 0.7 + anlik * 0.3 };
                son_deger = deger;
                son_an = simdi;
                let toplam = kayit_iz.ilerleme.toplam.load(Ordering::Relaxed);
                let detay = kayit_iz.ilerleme.detay.lock().unwrap().clone();
                let _ = olay_iz.send(Event::progress(
                    &id_iz,
                    deger,
                    toplam,
                    ema,
                    detay,
                ));
            }
        });

        id
    }

    fn bul(&self, id: &str) -> Option<Arc<Kayit>> {
        self.kayitlar.lock().unwrap().get(id).cloned()
    }

    pub fn komut(&self, k: Command) {
        match k {
            Command::Cancel { download_id } => {
                if let Some(k) = self.bul(&download_id) {
                    k.kontrol.iptal();
                }
            }
            Command::Pause { download_id } => {
                if let Some(k) = self.bul(&download_id) {
                    if *k.durum.lock().unwrap() == "indiriliyor" {
                        *k.durum.lock().unwrap() = "duraklatildi";
                        k.kontrol.duraklat();
                    }
                }
            }
            Command::Resume { download_id } => {
                if let Some(k) = self.bul(&download_id) {
                    if *k.durum.lock().unwrap() == "duraklatildi" {
                        *k.durum.lock().unwrap() = "indiriliyor";
                        k.kontrol.devam();
                    }
                }
            }
            Command::RefreshLink {
                download_id,
                url,
                headers,
            } => {
                if let Some(k) = self.bul(&download_id) {
                    *k.url.lock().unwrap() = url.clone();
                    k.kontrol.baglam_yaz(url, headers);
                    let _ = self.olay.send(Event::refreshed(&download_id));
                }
            }
            _ => {}
        }
    }

    pub fn anlik_liste(&self) -> Vec<JobSnapshot> {
        let kayitlar = self.kayitlar.lock().unwrap();
        kayitlar
            .values()
            .map(|k| JobSnapshot {
                download_id: k.kontrol.is_kimligi(),
                url: k.url.lock().unwrap().clone(),
                filename: k.dosya_adi.lock().unwrap().clone(),
                state: k.durum.lock().unwrap().to_string(),
                downloaded_bytes: k.ilerleme.indirilen.load(Ordering::Relaxed),
                total_bytes: k.ilerleme.toplam.load(Ordering::Relaxed),
                speed_bps: 0.0,
            })
            .collect()
    }
}

async fn gorev_yurut(
    kayit: Arc<Kayit>,
    kontrol: Arc<IsKontrol>,
    ilerleme: IlerlemeRef,
    olay: mpsc::UnboundedSender<Event>,
    eslik: usize,
) -> Result<PathBuf, String> {
    let client = istemci_kur(eslik)?;
    let url = kontrol.url_oku();
    let headers = kontrol.ham_basliklar();
    let bilgi = probe::kesfet(&client, &url, &headers).await?;

    let dizin = dirs::download_dir().unwrap_or_else(|| PathBuf::from("."));
    let ham_ad = kayit
        .dosya_adi
        .lock()
        .unwrap()
        .clone();
    let taban_ad = if ham_ad.is_empty() || ham_ad == "belirleniyor…" {
        bilgi
            .dosya_adi
            .clone()
            .or_else(|| filename_from_url(&bilgi.son_url))
            .unwrap_or_else(|| format!("indirme_{}", chrono_budak()))
    } else {
        ham_ad
    };
    let temiz_ad = sanitize_filename(&taban_ad);
    let nihai_ad = match bilgi.tur {
        Tur::Hls => format!("{}.m3u8", temiz_ad.trim_end_matches(".m3u8")),
        Tur::Dash => format!("{}.mpd", temiz_ad.trim_end_matches(".mpd")),
        Tur::Http => {
            if temiz_ad.rfind('.').is_none() {
                format!("{temiz_ad}{}", uzanti_turu(bilgi.icerik_turu.as_deref()))
            } else {
                temiz_ad.clone()
            }
        }
    };
    *kayit.dosya_adi.lock().unwrap() = nihai_ad.clone();
    let aday_yol = dizin.join(&nihai_ad);
    // Devam ettirme: aday yolda geçerli dergi varsa aynı yol kullanılır (unique_path'e gitmeden)
    let hedef_yol = match bilgi.boyut.filter(|_| bilgi.aralik_destekli) {
        Some(toplam) => {
            let dergi_uyumlu = Jurnal::yukle(&aday_yol)
                .ok()
                .flatten()
                .is_some_and(|(eski_toplam, araliklar)| {
                    eski_toplam == toplam && !araliklar.is_empty()
                });
            if dergi_uyumlu {
                aday_yol.clone()
            } else {
                unique_path(&dizin, &nihai_ad)
            }
        }
        None => unique_path(&dizin, &nihai_ad),
    };
    let _ = std::fs::create_dir_all(&dizin);

    match bilgi.tur {
        Tur::Hls => {
            let taban = temiz_ad.clone().trim_end_matches(".m3u8").to_string();
            crate::hls::hls_indir(
                client,
                kontrol,
                ilerleme,
                olay,
                &dizin,
                &taban,
            )
            .await
        }
        Tur::Dash => {
            let taban = temiz_ad.trim_end_matches(".mpd").to_string();
            crate::dash::dash_indir(client, kontrol, ilerleme, olay, &dizin, &taban).await
        }
        Tur::Http => {
            let Some(toplam) = bilgi.boyut.filter(|_| bilgi.aralik_destekli) else {
                let dosya = HedefDosya::olustur(&hedef_yol, 0).map_err(|e| e.to_string())?;
                ilerleme.toplam.store(0, Ordering::SeqCst);
                return match tek_akis_indir(client, kontrol, ilerleme, dosya, olay).await? {
                    Bitis::Tamamlandi => Ok(hedef_yol),
                    Bitis::Iptal => Err("İPTAL".into()),
                };
            };
            ilerleme.toplam.store(toplam, Ordering::SeqCst);
            let dosya = HedefDosya::olustur(&hedef_yol, toplam).map_err(|e| e.to_string())?;
            let jurnal = Jurnal::ac(&hedef_yol, toplam).map_err(|e| e.to_string())?;
            match paralel_indir(
                client,
                kontrol,
                ilerleme,
                dosya,
                Some(jurnal),
                toplam,
                eslik,
                olay,
            )
            .await?
            {
                Bitis::Tamamlandi => Ok(hedef_yol),
                Bitis::Iptal => Err("İPTAL".into()),
            }
        }
    }
}

fn chrono_budak() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
