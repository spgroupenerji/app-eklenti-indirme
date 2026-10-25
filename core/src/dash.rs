use crate::engine::{http_cek, paralel_indir, tek_akis_indir, IsKontrol, IlerlemeRef, Bitis};
use crate::protocol::Event;
use crate::probe;
use crate::remux::{ffmpeg_var, mux_iki, url_birlestir};
use crate::storage::HedefDosya;
use crate::util::{iso8601_duration_to_secs, sanitize_filename, HLS_CONCURRENCY, MAX_DASH_SEGMENTS, MAX_RETRIES};
use futures_util::stream::{self, StreamExt};
use quick_xml::events::Event as XmlEvent;
use quick_xml::Reader;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;

#[derive(Default, Clone)]
struct Sablon {
    timescale: Option<u64>,
    sure: Option<u64>,
    baslangic_no: Option<u64>,
    init: Option<String>,
    medya: Option<String>,
    cizelge: Vec<SElemani>,
}

#[derive(Default, Clone, Copy)]
struct SElemani {
    t: Option<u64>,
    d: u64,
    r: i64,
}

#[derive(Default, Clone)]
struct Liste {
    init_url: Option<String>,
    init_aralik: Option<(u64, u64)>,
    parcalar: Vec<(String, Option<(u64, u64)>)>,
}

#[derive(Default, Clone)]
struct TemsilHam {
    kimlik: String,
    bant: u64,
    en: Option<u64>,
    boy: Option<u64>,
    taban_url: Option<String>,
    sablon: Option<Sablon>,
    liste: Option<Liste>,
    tek_dosya: bool,
}

#[derive(Default)]
struct UyumSeti {
    video_mu: Option<bool>,
    taban_url: Option<String>,
    sablon: Option<Sablon>,
    temsiller: Vec<TemsilHam>,
}

struct Surec {
    taban_url: Option<String>,
    uyum_setleri: Vec<UyumSeti>,
    toplam_sure: Option<f64>,
    drm: bool,
}

fn attr_al(e: &quick_xml::events::BytesStart<'_>, ad: &str) -> Option<String> {
    e.attributes()
        .find_map(|a| {
            let a = a.ok()?;
            if a.key.local_name().as_ref() == ad {
                Some(a.value.to_string())
            } else {
                None
            }
        })
}

fn aralik_ayir(s: &str) -> Option<(u64, u64)> {
    let (b, e) = s.split_once('-')?;
    Some((b.parse().ok()?, e.parse().ok()?))
}

fn sablon_yap(e: &quick_xml::events::BytesStart<'_>) -> Sablon {
    Sablon {
        timescale: attr_al(e, "timescale").and_then(|v| v.parse().ok()),
        sure: attr_al(e, "duration").and_then(|v| v.parse().ok()),
        baslangic_no: attr_al(e, "startNumber").and_then(|v| v.parse().ok()),
        init: attr_al(e, "initialization"),
        medya: attr_al(e, "media"),
        cizelge: Vec::new(),
    }
}

fn liste_yap(e: &quick_xml::events::BytesStart<'_>) -> Liste {
    let _ = e;
    Liste::default()
}

fn mpd_ayristir(metin: &str) -> Result<Surec, String> {
    let mut okuyucu = Reader::from_str(metin);
    okuyucu.config_mut().trim_text(true);
    let mut surec = Surec {
        taban_url: None,
        uyum_setleri: Vec::new(),
        toplam_sure: None,
        drm: false,
    };
    let mut yol: Vec<String> = Vec::new();
    let mut metin_hedefi: Option<usize> = None; // BaseURL metni için derinlik işareti

    loop {
        match okuyucu.read_event() {
            Ok(XmlEvent::Start(e)) => {
                let ad = e.name().as_ref().to_string();
                match ad.as_str() {
                    "MPD" => {
                        if let Some(d) = attr_al(&e, "mediaPresentationDuration") {
                            surec.toplam_sure = iso8601_duration_to_secs(&d);
                        }
                        if let Some(t) = attr_al(&e, "type") {
                            if t == "dynamic" {
                                return Err("Dinamik (canlı) DASH akışları desteklenmiyor".into());
                            }
                        }
                    }
                    "BaseURL" => metin_hedefi = Some(yol.len()),
                    "Period" => {}
                    "AdaptationSet" => {
                        // DASH contentType "video" ve MIME "video/mp4" iki biçimi de desteklenir;
                        // text/image/altyazı setleri hiçbir kanala atanmaz.
                        let video_mu = attr_al(&e, "contentType")
                            .or_else(|| attr_al(&e, "mimeType"))
                            .and_then(|t| {
                                let ana =
                                    t.split('/').next().unwrap_or("").trim().to_ascii_lowercase();
                                match ana.as_str() {
                                    "video" => Some(true),
                                    "audio" => Some(false),
                                    _ => None,
                                }
                            });
                        surec.uyum_setleri.push(UyumSeti {
                            video_mu,
                            taban_url: None,
                            sablon: None,
                            temsiller: Vec::new(),
                        });
                    }
                    "Representation" => {
                        if let Some(set) = surec.uyum_setleri.last_mut() {
                            set.temsiller.push(TemsilHam {
                                kimlik: attr_al(&e, "id").unwrap_or_default(),
                                bant: attr_al(&e, "bandwidth").and_then(|v| v.parse().ok()).unwrap_or(0),
                                en: attr_al(&e, "width").and_then(|v| v.parse().ok()),
                                boy: attr_al(&e, "height").and_then(|v| v.parse().ok()),
                                taban_url: None,
                                sablon: None,
                                liste: None,
                                tek_dosya: false,
                            });
                        }
                    }
                    "ContentProtection" => {
                        let sema = attr_al(&e, "schemeIdUri")
                            .unwrap_or_default()
                            .to_ascii_lowercase();
                        let deger = attr_al(&e, "value").unwrap_or_default().to_ascii_lowercase();
                        let kid = attr_al(&e, "default_KID").is_some();
                        if sema.contains("cenc")
                            || sema.contains("mp4protection")
                            || deger.contains("cenc")
                            || kid
                        {
                            surec.drm = true;
                        }
                    }
                    "SegmentTemplate" => {
                        let s = sablon_yap(&e);
                        if yol.iter().any(|x| x == "Representation") {
                            if let Some(set) = surec.uyum_setleri.last_mut() {
                                if let Some(rep) = set.temsiller.last_mut() {
                                    rep.sablon = Some(s);
                                }
                            }
                        } else if let Some(set) = surec.uyum_setleri.last_mut() {
                            set.sablon = Some(s);
                        }
                    }
                    "SegmentList" => {
                        let mut l = liste_yap(&e);
                        if let Some(init) = attr_al(&e, "initialization") {
                            // initialization özniteliği SegmentList'te olmaz; child formu aşağıda
                            l.init_url = Some(init);
                        }
                        if yol.iter().any(|x| x == "Representation") {
                            if let Some(set) = surec.uyum_setleri.last_mut() {
                                if let Some(rep) = set.temsiller.last_mut() {
                                    rep.liste = Some(l);
                                }
                            }
                        } else if let Some(set) = surec.uyum_setleri.last_mut() {
                            // Set düzeyinde liste temsillere kopyalanır
                            for rep in &mut set.temsiller {
                                rep.liste = Some(l.clone());
                            }
                        }
                    }
                    "SegmentBase" => {
                        if let Some(set) = surec.uyum_setleri.last_mut() {
                            if let Some(rep) = set.temsiller.last_mut() {
                                rep.tek_dosya = true;
                            }
                        }
                    }
                    "Initialization" => {
                        let url = attr_al(&e, "sourceURL");
                        let aralik = attr_al(&e, "range").and_then(|r| aralik_ayir(&r));
                        if yol.iter().any(|x| x == "Representation") {
                            if let Some(set) = surec.uyum_setleri.last_mut() {
                                if let Some(rep) = set.temsiller.last_mut() {
                                    if let Some(l) = rep.liste.as_mut() {
                                        l.init_url = url;
                                        l.init_aralik = aralik;
                                    }
                                }
                            }
                        }
                    }
                    "SegmentURL" => {
                        let medya = attr_al(&e, "media");
                        let aralik = attr_al(&e, "mediaRange").and_then(|r| aralik_ayir(&r));
                        if let Some(set) = surec.uyum_setleri.last_mut() {
                            if let Some(rep) = set.temsiller.last_mut() {
                                if let Some(l) = rep.liste.as_mut() {
                                    l.parcalar.push((medya.unwrap_or_default(), aralik));
                                }
                            }
                        }
                    }
                    "S" => {
                        let s = SElemani {
                            t: attr_al(&e, "t").and_then(|v| v.parse().ok()),
                            d: attr_al(&e, "d").and_then(|v| v.parse().ok()).unwrap_or(0),
                            r: attr_al(&e, "r").and_then(|v| v.parse().ok()).unwrap_or(0),
                        };
                        if let Some(set) = surec.uyum_setleri.last_mut() {
                            let hedef = if yol.iter().any(|x| x == "Representation") {
                                set.temsiller.last_mut().and_then(|r| r.sablon.as_mut())
                            } else {
                                set.sablon.as_mut()
                            };
                            if let Some(sablon) = hedef {
                                sablon.cizelge.push(s);
                            }
                        }
                    }
                    _ => {}
                }
                yol.push(ad);
            }
            Ok(XmlEvent::Empty(e)) => {
                let ad = e.name().as_ref().to_string();
                if ad == "SegmentTemplate" {
                    let s = sablon_yap(&e);
                    if yol.iter().any(|x| x == "Representation") {
                        if let Some(set) = surec.uyum_setleri.last_mut() {
                            if let Some(rep) = set.temsiller.last_mut() {
                                rep.sablon = Some(s);
                            }
                        }
                    } else if let Some(set) = surec.uyum_setleri.last_mut() {
                        set.sablon = Some(s);
                    }
                } else if ad == "ContentProtection" {
                    let sema = attr_al(&e, "schemeIdUri")
                        .unwrap_or_default()
                        .to_ascii_lowercase();
                    let deger = attr_al(&e, "value").unwrap_or_default().to_ascii_lowercase();
                    let kid = attr_al(&e, "default_KID").is_some();
                    if sema.contains("cenc")
                        || sema.contains("mp4protection")
                        || deger.contains("cenc")
                        || kid
                    {
                        surec.drm = true;
                    }
                } else if ad == "SegmentBase" {
                    if let Some(set) = surec.uyum_setleri.last_mut() {
                        if let Some(rep) = set.temsiller.last_mut() {
                            rep.tek_dosya = true;
                        }
                    }
                } else if ad == "BaseURL" {
                    // Boş BaseURL anlamsızdır
                } else if ad == "SegmentURL" {
                    let medya = attr_al(&e, "media");
                    let aralik = attr_al(&e, "mediaRange").and_then(|r| aralik_ayir(&r));
                    if let Some(set) = surec.uyum_setleri.last_mut() {
                        if let Some(rep) = set.temsiller.last_mut() {
                            rep.liste
                                .get_or_insert_with(Liste::default)
                                .parcalar
                                .push((medya.unwrap_or_default(), aralik));
                        }
                    }
                } else if ad == "S" {
                    let s = SElemani {
                        t: attr_al(&e, "t").and_then(|v| v.parse().ok()),
                        d: attr_al(&e, "d").and_then(|v| v.parse().ok()).unwrap_or(0),
                        r: attr_al(&e, "r").and_then(|v| v.parse().ok()).unwrap_or(0),
                    };
                    if let Some(set) = surec.uyum_setleri.last_mut() {
                        let hedef = if yol.iter().any(|x| x == "Representation") {
                            set.temsiller.last_mut().and_then(|r| r.sablon.as_mut())
                        } else {
                            set.sablon.as_mut()
                        };
                        if let Some(sablon) = hedef {
                            sablon.cizelge.push(s);
                        }
                    }
                } else if ad == "Initialization" {
                    let url = attr_al(&e, "sourceURL");
                    let aralik = attr_al(&e, "range").and_then(|r| aralik_ayir(&r));
                    if let Some(set) = surec.uyum_setleri.last_mut() {
                        if let Some(rep) = set.temsiller.last_mut() {
                            let l = rep.liste.get_or_insert_with(Liste::default);
                            l.init_url = url;
                            l.init_aralik = aralik;
                        }
                    }
                }
            }
            Ok(XmlEvent::Text(t)) => {
                if let Some(hedef_derinlik) = metin_hedefi {
                    if yol.len() == hedef_derinlik + 1 && yol.last().map(|s| s.as_str()) == Some("BaseURL") {
                        let metin = t.as_ref().to_string();
                        let mut taban = surec.taban_url.clone();
                        fn uygula(eski: Option<String>, yeni: &str) -> Option<String> {
                            match eski {
                                Some(e) => url_birlestir(&e, yeni),
                                None => Some(yeni.to_string()),
                            }
                        }
                        if yol.iter().any(|x| x == "Representation") {
                            if let Some(set) = surec.uyum_setleri.last_mut() {
                                if let Some(rep) = set.temsiller.last_mut() {
                                    rep.taban_url = Some(match rep.taban_url.take().or_else(|| set.taban_url.clone().or_else(|| taban.take())) {
                                        Some(e) => url_birlestir(&e, &metin).unwrap_or(metin.clone()),
                                        None => metin.clone(),
                                    });
                                }
                            }
                        } else if yol.iter().any(|x| x == "AdaptationSet") {
                            if let Some(set) = surec.uyum_setleri.last_mut() {
                                set.taban_url = uygula(set.taban_url.clone().or_else(|| surec.taban_url.clone()), &metin);
                            }
                        } else if yol.iter().any(|x| x == "Period") {
                            let p = uygula(surec.taban_url.clone(), &metin);
                            if let Some(set) = surec.uyum_setleri.last_mut() {
                                set.taban_url = p;
                            } else {
                                surec.taban_url = p;
                            }
                        } else {
                            surec.taban_url = uygula(surec.taban_url.clone(), &metin);
                        }
                    }
                }
            }
            Ok(XmlEvent::End(_)) => {
                yol.pop();
                metin_hedefi = None;
            }
            Ok(XmlEvent::Eof) => break,
            Err(e) => return Err(format!("MPD ayrıştırılamadı: {e}")),
            _ => {}
        }
    }
    Ok(surec)
}

fn sablon_birlestir(ust: Option<&Sablon>, alt: Option<&Sablon>) -> Option<Sablon> {
    let (Some(a), Some(b)) = (ust, alt) else {
        return ust.cloned().or_else(|| alt.cloned());
    };
    Some(Sablon {
        timescale: b.timescale.or(a.timescale),
        sure: b.sure.or(a.sure),
        baslangic_no: b.baslangic_no.or(a.baslangic_no),
        init: b.init.clone().or_else(|| a.init.clone()),
        medya: b.medya.clone().or_else(|| a.medya.clone()),
        cizelge: if b.cizelge.is_empty() { a.cizelge.clone() } else { b.cizelge.clone() },
    })
}

fn sablon_genistir(
    sablon: &str,
    rep_id: &str,
    bant: u64,
    sayi: Option<u64>,
    zaman: Option<u64>,
) -> String {
    let bayt = sablon.as_bytes();
    let mut cikti = String::with_capacity(sablon.len() + 16);
    let mut i = 0;
    while i < bayt.len() {
        if bayt[i] != b'$' {
            cikti.push(bayt[i] as char);
            i += 1;
            continue;
        }
        if i + 1 < bayt.len() && bayt[i + 1] == b'$' {
            cikti.push('$');
            i += 2;
            continue;
        }
        let Some(kapanis) = bayt[i + 1..].iter().position(|&c| c == b'$') else {
            cikti.push('$');
            i += 1;
            continue;
        };
        let belirtec = &sablon[i + 1..i + 1 + kapanis];
        let deger = if belirtec == "RepresentationID" {
            rep_id.to_string()
        } else if belirtec == "Bandwidth" {
            bant.to_string()
        } else if belirtec == "Number" || belirtec.starts_with("Number%0") {
            let genislik = belirtec
                .strip_prefix("Number%0")
                .and_then(|k| k.strip_suffix('d'))
                .and_then(|k| k.parse::<usize>().ok())
                .unwrap_or(0);
            match sayi {
                Some(s) => {
                    let b = s.to_string();
                    if genislik > b.len() {
                        format!("{}{b}", "0".repeat(genislik - b.len()))
                    } else {
                        b
                    }
                }
                None => belirtec.to_string(),
            }
        } else if belirtec == "Time" {
            zaman.map(|z| z.to_string()).unwrap_or_else(|| belirtec.to_string())
        } else {
            belirtec.to_string()
        };
        cikti.push_str(&deger);
        i += kapanis + 2;
    }
    cikti
}

struct Kanal {
    taban: String,
    init: Option<(String, Option<(u64, u64)>)>,
    parcalar: Vec<(String, Option<(u64, u64)>)>,
    tek_dosya_url: Option<String>,
    sablon_sure: Option<(u64, u64, u64)>,
}

fn kanal_kur(
    mpd_url: &str,
    mpd_taban: Option<&str>,
    set_taban: Option<&str>,
    set_sablon: Option<&Sablon>,
    rep: &TemsilHam,
) -> Result<Kanal, String> {
    let kok = rep
        .taban_url
        .as_deref()
        .or(set_taban)
        .or(mpd_taban)
        .unwrap_or(mpd_url);
    let kok = url_birlestir(mpd_url, kok).unwrap_or_else(|| kok.to_string());

    if let Some(l) = &rep.liste {
        let init = l
            .init_url
            .as_ref()
            .map(|u| (url_birlestir(&kok, u).unwrap_or_else(|| u.clone()), l.init_aralik));
        let parcalar = l
            .parcalar
            .iter()
            .map(|(u, r)| (url_birlestir(&kok, u).unwrap_or_else(|| u.clone()), *r))
            .collect();
        return Ok(Kanal {
            taban: kok,
            init,
            parcalar,
            tek_dosya_url: None,
            sablon_sure: None,
        });
    }

    let birlesik_sablon = sablon_birlestir(set_sablon, rep.sablon.as_ref());
    if let Some(s) = birlesik_sablon.as_ref() {
        let bant = rep.bant;
        let rid = &rep.kimlik;
        let init = s.init.as_ref().map(|i| {
            let genis = sablon_genistir(i, rid, bant, None, None);
            (url_birlestir(&kok, &genis).unwrap_or(genis), None)
        });
        let medya = s.medya.clone().ok_or("SegmentTemplate media özniteliği eksik")?;
        if !s.cizelge.is_empty() {
            let mut parcalar = Vec::new();
            let mut t = 0u64;
            let mut numara = s.baslangic_no.unwrap_or(1);
            for ele in &s.cizelge {
                if ele.r < 0 {
                    return Err(
                        "SegmentTimeline'da r<0 (açık uçlu tekrar) desteklenmiyor".into(),
                    );
                }
                let bas = ele.t.unwrap_or(t);
                let tekrar = ele.r as u64 + 1;
                for k in 0..tekrar {
                    let zaman = bas + k * ele.d;
                    let genis = sablon_genistir(&medya, rid, bant, Some(numara), Some(zaman));
                    parcalar.push((url_birlestir(&kok, &genis).unwrap_or(genis), None));
                    numara += 1;
                }
                t = bas + tekrar * ele.d;
            }
            return Ok(Kanal {
                taban: kok,
                init,
                parcalar,
                tek_dosya_url: None,
                sablon_sure: None,
            });
        }
        if let Some(sure) = s.sure {
            let timescale = s.timescale.unwrap_or(1).max(1);
            return Ok(Kanal {
                taban: kok,
                init,
                parcalar: Vec::new(),
                tek_dosya_url: None,
                sablon_sure: Some((sure, timescale, s.baslangic_no.unwrap_or(1))),
            });
        }
        return Err("SegmentTemplate'te süre ya da zaman çizelgesi yok".into());
    }

    if rep.tek_dosya || rep.taban_url.is_some() {
        return Ok(Kanal {
            taban: kok.clone(),
            init: None,
            parcalar: Vec::new(),
            tek_dosya_url: Some(kok),
            sablon_sure: None,
        });
    }
    Err("Temsil için indirme kaynağı bulunamadı".to_string())
}

pub async fn dash_indir(
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
    let surec = mpd_ayristir(&metin)?;
    if surec.drm {
        return Err("Bu DASH akışı DRM (CENC) ile korunuyor; indirilemez".into());
    }
    let taban = sanitize_filename(istenen_ad.trim_end_matches(".mpd"));
    let video_set = surec.uyum_setleri.iter().find(|s| s.video_mu == Some(true));
    let ses_set = surec.uyum_setleri.iter().find(|s| s.video_mu == Some(false));
    if video_set.is_none() && ses_set.is_none() {
        return Err("MPD içinde video/ses uyum seti bulunamadı".into());
    }

    ilerleme.toplam.store(0, Ordering::SeqCst);
    let kok_dizin = dizin.to_path_buf();

    let mut video_yolu: Option<PathBuf> = None;
    if let Some(set) = video_set {
        let en_iyi = set
            .temsiller
            .iter()
            .max_by_key(|r| (r.en.unwrap_or(0) * r.boy.unwrap_or(0), r.bant))
            .ok_or("Video setinde temsil yok")?;
        let mut kanal = kanal_kur(&url, surec.taban_url.as_deref(), set.taban_url.as_deref(), set.sablon.as_ref(), en_iyi)?;
        if let Some((sure, timescale, bas_no)) = kanal.sablon_sure.take() {
            kanal = sablon_sure_doldur(
                kanal,
                en_iyi,
                surec.toplam_sure,
                sure,
                timescale,
                bas_no,
            )?;
        }
        let yol = kok_dizin.join(format!("{taban}.video.mp4"));
        kanal_indir(&client, &kontrol, &ilerleme, &olay, &kanal, &yol, "video").await?;
        video_yolu = Some(yol);
    }

    let mut ses_yolu: Option<PathBuf> = None;
    if let Some(set) = ses_set {
        let en_iyi = set
            .temsiller
            .iter()
            .max_by_key(|r| r.bant)
            .ok_or("Ses setinde temsil yok")?;
        let mut kanal = kanal_kur(&url, surec.taban_url.as_deref(), set.taban_url.as_deref(), set.sablon.as_ref(), en_iyi)?;
        if let Some((sure, timescale, bas_no)) = kanal.sablon_sure.take() {
            kanal = sablon_sure_doldur(
                kanal,
                en_iyi,
                surec.toplam_sure,
                sure,
                timescale,
                bas_no,
            )?;
        }
        let yol = kok_dizin.join(format!("{taban}.audio.m4a"));
        kanal_indir(&client, &kontrol, &ilerleme, &olay, &kanal, &yol, "ses").await?;
        ses_yolu = Some(yol);
    }

    let cikis = kok_dizin.join(format!("{taban}.mp4"));
    match (&video_yolu, &ses_yolu) {
        (Some(v), Some(a)) => {
            if ffmpeg_var() {
                mux_iki(v, Some(a), &cikis).await?;
                let _ = std::fs::remove_file(v);
                let _ = std::fs::remove_file(a);
                Ok(cikis)
            } else {
                ilerleme.detay_yaz("ffmpeg bulunamadı; video ve ses ayrı dosyalarda".into());
                Ok(v.clone())
            }
        }
        (Some(v), None) => {
            if ffmpeg_var() {
                mux_iki(v, None, &cikis).await?;
                let _ = std::fs::remove_file(v);
                Ok(cikis)
            } else {
                Ok(v.clone())
            }
        }
        (None, Some(a)) => Ok(a.clone()),
        (None, None) => Err("İndirilecek kanal kalmadı".into()),
    }
}

#[allow(clippy::type_complexity)]
fn sablon_sure_doldur(
    mut kanal: Kanal,
    rep: &TemsilHam,
    toplam_sure: Option<f64>,
    sure: u64,
    timescale: u64,
    bas_no: u64,
) -> Result<Kanal, String> {
    let toplam = toplam_sure.ok_or("SegmentTemplate@duration için toplam süre gerekli")?;
    if sure == 0 {
        return Err("SegmentTemplate@duration sıfır olamaz".into());
    }
    let seg_sure = sure as f64 / timescale.max(1) as f64;
    let adet = (toplam / seg_sure).ceil() as u64;
    if adet == 0 || adet > MAX_DASH_SEGMENTS {
        return Err(format!("SegmentTemplate parça sayısı geçersiz: {adet}"));
    }
    let medya_sablon = sablon_birlestir(None, rep.sablon.as_ref())
        .and_then(|s| s.medya.clone())
        .ok_or("media şablonu eksik")?;
    let mut parcalar = Vec::with_capacity(adet as usize);
    for k in 0..adet {
        let genis = sablon_genistir(&medya_sablon, &rep.kimlik, rep.bant, Some(bas_no + k), None);
        parcalar.push((url_birlestir(&kanal.taban, &genis).unwrap_or(genis), None));
    }
    kanal.parcalar = parcalar;
    Ok(kanal)
}

async fn kanal_indir(
    client: &reqwest::Client,
    kontrol: &Arc<IsKontrol>,
    ilerleme: &IlerlemeRef,
    olay: &tokio::sync::mpsc::UnboundedSender<Event>,
    kanal: &Kanal,
    hedef: &std::path::Path,
    etiket: &str,
) -> Result<(), String> {
    if let Some(tek) = &kanal.tek_dosya_url {
        // SegmentBase: temsil tek başına tam bir dosyadır; paralel motorla indirilir
        let (_, basliklar) = kontrol.baglam_oku();
        let mut harita = std::collections::HashMap::new();
        for (k, v) in basliklar.iter() {
            if let Ok(v_str) = v.to_str() {
                harita.insert(k.as_str().to_string(), v_str.to_string());
            }
        }
        let bilgi = probe::kesfet(client, tek, &harita).await?;
        let toplam = bilgi.boyut.ok_or("SegmentBase dosya boyutu bilinmiyor")?;
        if !bilgi.aralik_destekli {
            let dosya = HedefDosya::olustur(hedef, 0).map_err(|e| e.to_string())?;
            let ileri = IlerlemeRef::clone(ilerleme);
            match tek_akis_indir(client.clone(), kontrol.clone(), ileri, dosya, olay.clone()).await? {
                Bitis::Tamamlandi => return Ok(()),
                Bitis::Iptal => return Err("İPTAL".into()),
            }
        }
        let dosya = HedefDosya::olustur(hedef, toplam).map_err(|e| e.to_string())?;
        let jurnal = crate::storage::Jurnal::ac(hedef, toplam).map_err(|e| e.to_string())?;
        let ileri = IlerlemeRef::clone(ilerleme);
        ileri.toplam.store(toplam, Ordering::SeqCst);
        match paralel_indir(
            client.clone(),
            kontrol.clone(),
            ileri,
            dosya,
            Some(jurnal),
            toplam,
            8,
            olay.clone(),
        )
        .await?
        {
            Bitis::Tamamlandi => return Ok(()),
            Bitis::Iptal => return Err("İPTAL".into()),
        }
    }

    use tokio::io::AsyncWriteExt;
    let mut dosya = tokio::fs::File::create(hedef)
        .await
        .map_err(|e| format!("Kanal dosyası açılamadı: {e}"))?;
    if let Some((url, aralik)) = &kanal.init {
        let veri = http_cek(client, kontrol, olay, url, *aralik, MAX_RETRIES).await?;
        dosya.write_all(&veri).await.map_err(|e| e.to_string())?;
    }
    let toplam = kanal.parcalar.len() as u64;
    ilerleme.detay_yaz(format!("{etiket} 0/{toplam} parça"));
    let mut akis = stream::iter(kanal.parcalar.clone())
        .map(|(url, aralik)| {
            let client = client.clone();
            let kontrol = kontrol.clone();
            let olay = olay.clone();
            async move { http_cek(&client, &kontrol, &olay, &url, aralik, MAX_RETRIES).await }
        })
        .buffered(HLS_CONCURRENCY);
    let mut i: u64 = 0;
    while let Some(sonuc) = akis.next().await {
        kontrol.kontrol_noktasi().await?;
        let veri = sonuc?;
        dosya.write_all(&veri).await.map_err(|e| e.to_string())?;
        i += 1;
        ilerleme.indirilen.fetch_add(veri.len() as u64, Ordering::Relaxed);
        ilerleme.detay_yaz(format!("{etiket} {i}/{toplam} parça"));
    }
    dosya.sync_data().await.map_err(|e| e.to_string())
}

#[cfg(test)]
mod testler {
    use super::*;

    #[test]
    fn ffmpeg_mpd_ayristirma() {
        let yol = concat!(env!("CARGO_MANIFEST_DIR"), "/../tests/fixture/dash/sunum.mpd");
        if !std::path::Path::new(yol).exists() {
            eprintln!("tests/fixture/dash/sunum.mpd yok; test atlandı (önce python tests/fixture_uret.py)");
            return;
        }
        let metin = std::fs::read_to_string(yol).expect("fixture MPD okunamadı");
        let surec = mpd_ayristir(&metin).expect("ayrıştırma hatası");
        eprintln!("toplam_sure={:?}", surec.toplam_sure);
        for s in &surec.uyum_setleri {
            eprintln!(
                "set video_mu={:?} taban={:?} sablon_var={} temsil={}",
                s.video_mu,
                s.taban_url,
                s.sablon.is_some(),
                s.temsiller.len()
            );
            for r in &s.temsiller {
                eprintln!(
                    "  rep id={} bant={} tek_dosya={} sablon={:?} liste_var={}",
                    r.kimlik, r.bant, r.tek_dosya, r.sablon.is_some(), r.liste.is_some()
                );
            }
        }
        assert_eq!(surec.uyum_setleri.len(), 2);
        assert!(surec.uyum_setleri.iter().any(|s| s.video_mu == Some(true)));
        assert!(surec.uyum_setleri.iter().any(|s| s.video_mu == Some(false)));
        assert!(surec.toplam_sure.is_some());
    }
}
