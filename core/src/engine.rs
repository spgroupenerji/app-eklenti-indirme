use crate::probe::basliklara_cevir;
use crate::storage::{aralik_birlestir, tumleyen, HedefDosya, Jurnal};
use crate::util::*;
use futures_util::StreamExt;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use tokio::sync::Notify;

pub struct Baglam {
    pub url: String,
    pub headers: HashMap<String, String>,
}

pub struct IsKontrol {
    kimlik: String,
    iptal: AtomicBool,
    duraklat: AtomicBool,
    yenileme_bekliyor: AtomicBool,
    baglam: RwLock<Baglam>,
    uyandir: Notify,
}

impl IsKontrol {
    pub fn yeni(kimlik: String, url: String, headers: HashMap<String, String>) -> Arc<Self> {
        Arc::new(Self {
            kimlik,
            iptal: AtomicBool::new(false),
            duraklat: AtomicBool::new(false),
            yenileme_bekliyor: AtomicBool::new(false),
            baglam: RwLock::new(Baglam { url, headers }),
            uyandir: Notify::new(),
        })
    }
    pub fn is_kimligi(&self) -> String {
        self.kimlik.clone()
    }
    pub fn iptal(&self) {
        self.iptal.store(true, Ordering::SeqCst);
        self.uyandir.notify_waiters();
    }
    pub fn iptal_mi(&self) -> bool {
        self.iptal.load(Ordering::SeqCst)
    }
    pub fn duraklat(&self) {
        self.duraklat.store(true, Ordering::SeqCst);
    }
    pub fn devam(&self) {
        self.duraklat.store(false, Ordering::SeqCst);
        self.uyandir.notify_waiters();
    }
    pub fn duraklat_mi(&self) -> bool {
        self.duraklat.load(Ordering::SeqCst)
    }
    pub fn baglam_oku(&self) -> (String, HeaderKaydi) {
        let b = self.baglam.read().unwrap();
        (b.url.clone(), basliklara_cevir(&b.headers))
    }
    pub fn url_oku(&self) -> String {
        self.baglam.read().unwrap().url.clone()
    }
    pub fn ham_basliklar(&self) -> HashMap<String, String> {
        self.baglam.read().unwrap().headers.clone()
    }
    pub fn baglam_yaz(&self, url: String, headers: HashMap<String, String>) {
        {
            let mut b = self.baglam.write().unwrap();
            b.url = url;
            b.headers = headers;
        }
        self.yenileme_bekliyor.store(false, Ordering::SeqCst);
        self.uyandir.notify_waiters();
    }
    async fn bekle_uyan(&self) {
        self.uyandir.notified().await;
    }
    pub async fn kontrol_noktasi(&self) -> Result<(), String> {
        if self.iptal_mi() {
            return Err("İPTAL".into());
        }
        while self.duraklat_mi() && !self.iptal_mi() {
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }
        if self.iptal_mi() {
            return Err("İPTAL".into());
        }
        Ok(())
    }
}

type HeaderKaydi = reqwest::header::HeaderMap;

pub type IlerlemeRef = Arc<Ilerleme>;

pub struct Ilerleme {
    pub indirilen: AtomicU64,
    pub toplam: AtomicU64,
    pub detay: Mutex<Option<String>>,
}

impl Ilerleme {
    pub fn yeni(toplam: u64, bastan: u64) -> Arc<Self> {
        Arc::new(Self {
            indirilen: AtomicU64::new(bastan),
            toplam: AtomicU64::new(toplam),
            detay: Mutex::new(None),
        })
    }
    pub fn detay_yaz(&self, metin: String) {
        *self.detay.lock().unwrap() = Some(metin);
    }
}

#[derive(Clone, Copy, Debug)]
struct Kesit {
    bas: u64,
    bit: u64,
}
impl Kesit {
    fn uzunluk(self) -> u64 {
        self.bit - self.bas
    }
}

struct AktifKesit {
    suan: Arc<AtomicU64>,
    bit: Arc<AtomicU64>,
}

pub struct Ihale {
    id: u64,
    pub bas: u64,
    suan: Arc<AtomicU64>,
    bit: Arc<AtomicU64>,
}

struct Ic {
    beklemede: Vec<Kesit>,
    aktif: Vec<(u64, AktifKesit)>,
    son_id: u64,
}

pub struct KesitYoneticisi {
    ic: Mutex<Ic>,
}

impl KesitYoneticisi {
    pub fn yeni(toplam: u64, kalan: &[(u64, u64)]) -> Self {
        let bos = kalan.is_empty();
        let kesitler = if bos {
            vec![Kesit {
                bas: 0,
                bit: toplam,
            }]
        } else {
            kalan.iter().map(|(b, e)| Kesit { bas: *b, bit: *e }).collect()
        };
        Self {
            ic: Mutex::new(Ic {
                beklemede: kesitler,
                aktif: Vec::new(),
                son_id: 0,
            }),
        }
    }

    fn ihale_kur(ic: &mut Ic, bas: u64, bit: u64) -> Ihale {
        ic.son_id += 1;
        let suan = Arc::new(AtomicU64::new(bas));
        let bit_arc = Arc::new(AtomicU64::new(bit));
        ic.aktif.push((
            ic.son_id,
            AktifKesit {
                suan: suan.clone(),
                bit: bit_arc.clone(),
            },
        ));
        Ihale {
            id: ic.son_id,
            bas,
            suan,
            bit: bit_arc,
        }
    }

    pub fn ihale_al(&self) -> Option<Ihale> {
        let mut ic = self.ic.lock().unwrap();
        ic.beklemede.sort_unstable_by(|a, b| b.uzunluk().cmp(&a.uzunluk()));
        if !ic.beklemede.is_empty() {
            let k = ic.beklemede.remove(0);
            if k.uzunluk() > STEAL_THRESHOLD {
                let orta = k.bas + k.uzunluk() / 2;
                ic.beklemede.push(Kesit { bas: orta, bit: k.bit });
                return Some(Self::ihale_kur(&mut ic, k.bas, orta));
            }
            return Some(Self::ihale_kur(&mut ic, k.bas, k.bit));
        }
        // İş çalma: en çok baytı kalan aktif kesit ortasından bölünür
        loop {
            let aday_idx = ic
                .aktif
                .iter()
                .enumerate()
                .filter(|(_, (_, a))| a.bit.load(Ordering::SeqCst) > a.suan.load(Ordering::SeqCst))
                .max_by_key(|(_, (_, a))| a.bit.load(Ordering::SeqCst) - a.suan.load(Ordering::SeqCst))
                .map(|(idx, _)| idx);
            let Some(idx) = aday_idx else { return None };
            let (suan_v, bit_v) = {
                let a = &ic.aktif[idx].1;
                (a.suan.load(Ordering::SeqCst), a.bit.load(Ordering::SeqCst))
            };
            let kalan = bit_v.saturating_sub(suan_v);
            if kalan <= STEAL_THRESHOLD {
                return None;
            }
            let orta = suan_v + kalan / 2;
            let cas_basarili = ic.aktif[idx]
                .1
                .bit
                .compare_exchange(bit_v, orta, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok();
            if cas_basarili {
                return Some(Self::ihale_kur(&mut ic, orta, bit_v));
            }
            // CAS yarış kaybetti: yeniden dene
        }
    }

    pub fn bitir(&self, id: u64) {
        self.ic.lock().unwrap().aktif.retain(|(i, _)| *i != id);
    }

    pub fn bos_mu(&self) -> bool {
        let ic = self.ic.lock().unwrap();
        ic.beklemede.is_empty() && ic.aktif.is_empty()
    }
}

pub enum Bitis {
    Tamamlandi,
    Iptal,
}

pub async fn paralel_indir(
    client: reqwest::Client,
    kontrol: Arc<IsKontrol>,
    ilerleme: Arc<Ilerleme>,
    dosya: HedefDosya,
    jurnal: Option<Jurnal>,
    toplam: u64,
    eslik: usize,
    olay: tokio::sync::mpsc::UnboundedSender<crate::protocol::Event>,
) -> Result<Bitis, String> {
    let (devam_kalan, bastan) = match jurnal
        .as_ref()
        .and_then(|_| Jurnal::yukle(&dosya.yol).ok().flatten())
    {
        Some((eski_toplam, araliklar)) if eski_toplam == toplam && !araliklar.is_empty() => {
            let birlesik = aralik_birlestir(araliklar);
            let tamamlanan: u64 = birlesik.iter().map(|(b, e)| e - b).sum();
            (tumleyen(toplam, &birlesik), tamamlanan)
        }
        _ => (vec![(0, toplam)], 0),
    };
    ilerleme.indirilen.store(bastan, Ordering::SeqCst);

    let yonetici = Arc::new(KesitYoneticisi::yeni(toplam, &devam_kalan));
    let dosya_arc = Arc::new(tokio::sync::Mutex::new(dosya));
    let jurnal_arc = Arc::new(tokio::sync::Mutex::new(jurnal));

    let mut gorevler = Vec::with_capacity(eslik);
    for _ in 0..eslik {
        let y = yonetici.clone();
        let k = kontrol.clone();
        let i = ilerleme.clone();
        let d = dosya_arc.clone();
        let j = jurnal_arc.clone();
        let c = client.clone();
        let o = olay.clone();
        gorevler.push(tokio::spawn(isci(c, y, k, i, d, j, o)));
    }
    let mut isci_hatasi: Option<String> = None;
    let mut iptal_edildi = false;
    for g in gorevler {
        match g.await {
            Ok(Ok(())) => {}
            Ok(Err(e)) if e == "İPTAL" => {
                kontrol.iptal();
                iptal_edildi = true;
            }
            Ok(Err(e)) => {
                kontrol.iptal();
                isci_hatasi.get_or_insert(e);
            }
            Err(e) => {
                kontrol.iptal();
                isci_hatasi.get_or_insert_with(|| format!("İşçi görevi çöktü: {e}"));
            }
        }
    }
    if let Some(hata) = isci_hatasi {
        return Err(hata);
    }
    if iptal_edildi {
        return Ok(Bitis::Iptal);
    }
    if !yonetici.bos_mu() {
        return Err("Kesit yönetimi tutarsız durumda bitti".into());
    }
    dosya_arc.lock().await.senkron().await.map_err(|e| e.to_string())?;
    if let Some(j) = jurnal_arc.lock().await.as_ref() {
        j.kaldir();
    }
    Ok(Bitis::Tamamlandi)
}

async fn isci(
    client: reqwest::Client,
    yonetici: Arc<KesitYoneticisi>,
    kontrol: Arc<IsKontrol>,
    ilerleme: Arc<Ilerleme>,
    dosya: Arc<tokio::sync::Mutex<HedefDosya>>,
    jurnal: Arc<tokio::sync::Mutex<Option<Jurnal>>>,
    olay: tokio::sync::mpsc::UnboundedSender<crate::protocol::Event>,
) -> Result<(), String> {
    let stall = std::time::Duration::from_secs(STALL_TIMEOUT_SECS);
    loop {
        let Some(ih) = yonetici.ihale_al() else {
            return Ok(());
        };
        loop {
            kontrol.kontrol_noktasi().await?;
            let bit = ih.bit.load(Ordering::SeqCst);
            let bas = ih.suan.load(Ordering::SeqCst);
            if bas >= bit {
                break;
            }
            let hedef = (bas + WINDOW_BYTES).min(bit);
            pencere_indir(
                &client, &kontrol, &ilerleme, &dosya, &ih, bas, hedef, &olay, stall,
            )
            .await?;
        }
        let son_bit = ih.bit.load(Ordering::SeqCst);
        dosya.lock().await.senkron().await.map_err(|e| e.to_string())?;
        if let Some(j) = jurnal.lock().await.as_mut() {
            let _ = j.aralik_ekle(ih.bas, son_bit);
        }
        yonetici.bitir(ih.id);
    }
}

#[allow(clippy::too_many_arguments)]
async fn pencere_indir(
    client: &reqwest::Client,
    kontrol: &Arc<IsKontrol>,
    ilerleme: &Arc<Ilerleme>,
    dosya: &Arc<tokio::sync::Mutex<HedefDosya>>,
    ih: &Ihale,
    bas: u64,
    hedef: u64,
    olay: &tokio::sync::mpsc::UnboundedSender<crate::protocol::Event>,
    stall: std::time::Duration,
) -> Result<(), String> {
    let mut deneme: u32 = 0;
    let mut konum = bas;
    loop {
        kontrol.kontrol_noktasi().await?;
        if deneme >= MAX_RETRIES {
            return Err(format!("Bağlantı {MAX_RETRIES} deneme sonrasında da kurulamadı"));
        }
        let (url, basliklar) = kontrol.baglam_oku();
        let istek = client
            .get(&url)
            .headers(basliklar)
            .header("Range", format!("bytes={konum}-{}", hedef - 1));
        match tokio::time::timeout(stall, istek.send()).await {
            Ok(Ok(yanit)) => {
                let durum = yanit.status();
                if durum.as_u16() == 401 || durum.as_u16() == 403 || durum.as_u16() == 410 {
                    yenileme_bekle(kontrol, olay).await?;
                    continue;
                }
                if durum.as_u16() == 429 || durum.is_server_error() {
                    geri_cekil(deneme).await;
                    deneme += 1;
                    continue;
                }
                if !durum.is_success() {
                    return Err(format!("Sunucu {durum} yanıtı verdi"));
                }
                if durum.as_u16() == 200 && konum > 0 {
                    return Err("Sunucu Range başlığını yoksaydı; indirme yeniden başlatılmalı".into());
                }
                let mut akis = yanit.bytes_stream();
                let tamamlanan = loop {
                    if kontrol.iptal_mi() {
                        return Err("İPTAL".into());
                    }
                    match tokio::time::timeout(stall, akis.next()).await {
                        Ok(Some(Ok(parca))) => {
                            let mut veri: &[u8] = &parca;
                            if konum + veri.len() as u64 > hedef {
                                veri = &veri[..(hedef - konum) as usize];
                            }
                            if !veri.is_empty() {
                                dosya
                                    .lock()
                                    .await
                                    .yaz_ofset(konum, veri)
                                    .await
                                    .map_err(|e| e.to_string())?;
                                ilerleme
                                    .indirilen
                                    .fetch_add(veri.len() as u64, Ordering::Relaxed);
                                konum += veri.len() as u64;
                                ih.suan.store(konum, Ordering::SeqCst);
                            }
                            if konum >= hedef {
                                break true;
                            }
                        }
                        Ok(Some(Err(_))) | Err(_) => break false,
                        Ok(None) => break konum >= hedef,
                    }
                };
                if tamamlanan {
                    return Ok(());
                }
                geri_cekil(deneme).await;
                deneme += 1;
                ih.suan.store(konum, Ordering::SeqCst);
            }
            Ok(Err(_)) | Err(_) => {
                geri_cekil(deneme).await;
                deneme += 1;
            }
        }
    }
}

async fn geri_cekil(deneme: u32) {
    let bekleme = RETRY_BASE_MS.saturating_mul(1 << deneme.min(5)).min(RETRY_MAX_MS);
    tokio::time::sleep(std::time::Duration::from_millis(bekleme)).await;
}

async fn yenileme_bekle(
    kontrol: &Arc<IsKontrol>,
    olay: &tokio::sync::mpsc::UnboundedSender<crate::protocol::Event>,
) -> Result<(), String> {
    if !kontrol.yenileme_bekliyor.swap(true, Ordering::SeqCst) {
        let _ = olay.send(crate::protocol::Event::refresh_required(&kontrol.is_kimligi()));
    }
    let son = tokio::time::Instant::now()
        + std::time::Duration::from_secs(REFRESH_TIMEOUT_SECS);
    loop {
        if kontrol.iptal_mi() {
            return Err("İPTAL".into());
        }
        if !kontrol.yenileme_bekliyor.load(Ordering::SeqCst) {
            return Ok(());
        }
        if tokio::time::Instant::now() >= son {
            return Err("Bağlantı süresi doldu ve yenilenmedi (bağlantı adresi geçersiz olabilir)".into());
        }
        tokio::select! {
            _ = kontrol.bekle_uyan() => {}
            _ = tokio::time::sleep(std::time::Duration::from_millis(250)) => {}
        }
    }
}

// HLS/DASH için genel amaçlı tek seferlik HTTP çekimi: yenileme + geri çekilme dahil
pub async fn http_cek(
    client: &reqwest::Client,
    kontrol: &Arc<IsKontrol>,
    olay: &tokio::sync::mpsc::UnboundedSender<crate::protocol::Event>,
    url: &str,
    aralik: Option<(u64, u64)>,
    max_deneme: u32,
) -> Result<Vec<u8>, String> {
    let stall = std::time::Duration::from_secs(STALL_TIMEOUT_SECS);
    let mut deneme: u32 = 0;
    loop {
        kontrol.kontrol_noktasi().await?;
        if deneme >= max_deneme {
            return Err(format!("Parça indirilemedi: {url}"));
        }
        let (baglam_url, basliklar) = kontrol.baglam_oku();
        // Akış adresi güncellenmiş olsa bile parametreli URL aynen kullanılır
        let _ = baglam_url;
        let mut istek = client.get(url).headers(basliklar);
        if let Some((b, e)) = aralik {
            istek = istek.header("Range", format!("bytes={b}-{e}"));
        }
        match tokio::time::timeout(stall, istek.send()).await {
            Ok(Ok(yanit)) => {
                let durum = yanit.status();
                if durum.as_u16() == 401 || durum.as_u16() == 403 || durum.as_u16() == 410 {
                    yenileme_bekle(kontrol, olay).await?;
                    continue;
                }
                if durum.as_u16() == 429 || durum.is_server_error() || !durum.is_success() {
                    geri_cekil(deneme).await;
                    deneme += 1;
                    continue;
                }
                let aralikli_istek = aralik.map_or(false, |(b, _)| b > 0);
                if aralikli_istek && durum.as_u16() != 206 {
                    geri_cekil(deneme).await;
                    deneme += 1;
                    continue;
                }
                match tokio::time::timeout(stall, yanit.bytes()).await {
                    Ok(Ok(govde)) => {
                        if let Some((b, e)) = aralik {
                            let beklenen = (e - b + 1) as usize;
                            if govde.len() >= beklenen {
                                return Ok(govde[..beklenen].to_vec());
                            }
                        }
                        return Ok(govde.to_vec());
                    }
                    _ => {
                        geri_cekil(deneme).await;
                        deneme += 1;
                    }
                }
            }
            _ => {
                geri_cekil(deneme).await;
                deneme += 1;
            }
        }
    }
}

pub async fn tek_akis_indir(
    client: reqwest::Client,
    kontrol: Arc<IsKontrol>,
    ilerleme: Arc<Ilerleme>,
    dosya: HedefDosya,
    olay: tokio::sync::mpsc::UnboundedSender<crate::protocol::Event>,
) -> Result<Bitis, String> {
    let stall = std::time::Duration::from_secs(STALL_TIMEOUT_SECS);
    let mut deneme: u32 = 0;
    let mut konum: u64 = 0;
    loop {
        kontrol.kontrol_noktasi().await?;
        if deneme >= MAX_RETRIES {
            return Err(format!("Bağlantı {MAX_RETRIES} deneme sonrasında da kurulamadı"));
        }
        let (url, basliklar) = kontrol.baglam_oku();
        let mut istek = client.get(&url).headers(basliklar);
        if konum > 0 {
            istek = istek.header("Range", format!("bytes={konum}-"));
        }
        match tokio::time::timeout(stall, istek.send()).await {
            Ok(Ok(yanit)) => {
                let durum = yanit.status();
                if durum.as_u16() == 401 || durum.as_u16() == 403 || durum.as_u16() == 410 {
                    yenileme_bekle(&kontrol, &olay).await?;
                    continue;
                }
                if durum.as_u16() == 429 || durum.is_server_error() {
                    geri_cekil(deneme).await;
                    deneme += 1;
                    continue;
                }
                if !durum.is_success() {
                    return Err(format!("Sunucu {durum} yanıtı verdi"));
                }
                if konum > 0 && durum.as_u16() != 206 {
                    return Err("Sunucu Range başlığını yoksaydı; indirme yeniden başlatılmalı".into());
                }
                let mut akis = yanit.bytes_stream();
                let mut duraklatildi = false;
                loop {
                    if kontrol.iptal_mi() {
                        return Err("İPTAL".into());
                    }
                    if kontrol.duraklat_mi() {
                        kontrol.kontrol_noktasi().await?;
                        duraklatildi = true;
                        break;
                    }
                    match tokio::time::timeout(stall, akis.next()).await {
                        Ok(Some(Ok(parca))) => {
                            dosya
                                .yaz_ofset(konum, &parca)
                                .await
                                .map_err(|e| e.to_string())?;
                            ilerleme
                                .indirilen
                                .fetch_add(parca.len() as u64, Ordering::Relaxed);
                            konum += parca.len() as u64;
                        }
                        Ok(Some(Err(_))) | Err(_) => break,
                        Ok(None) => {
                            dosya.senkron().await.map_err(|e| e.to_string())?;
                            return Ok(Bitis::Tamamlandi);
                        }
                    }
                }
                if duraklatildi {
                    deneme = 0;
                    continue;
                }
                geri_cekil(deneme).await;
                deneme += 1;
            }
            Ok(Err(_)) | Err(_) => {
                geri_cekil(deneme).await;
                deneme += 1;
            }
        }
    }
}
