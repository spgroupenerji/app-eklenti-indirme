// Titanium İndirme Hızlandırıcı — Arka Plan Hizmet İşçisi

const NATIVE_ADI = "com.titan.downloader";
const YENIDEN_BAGLANMA_MS = 5000;
const BEKCI_PERIYOT_DAKIKA = 0.5;
const YAKALANAN_SINIR = 100;

let port = null;
let portHataMesaji = "";
const istekBaglami = new Map(); // url -> { referer, origin }

// ---------- Native Messaging bağlantısı ----------

function baglan() {
  if (port) return;
  try {
    port = chrome.runtime.connectNative(NATIVE_ADI);
  } catch (e) {
    port = null;
    rozetGuncelle();
    return;
  }
  portHataMesaji = "";
  port.onMessage.addListener(portMesaji);
  port.onDisconnect.addListener(() => {
    const hata = chrome.runtime.lastError?.message || "";
    port = null;
    portHataMesaji = hata;
    rozetGuncelle();
    // Kurulum sonrası host kaydı görünene kadar sessiz yeniden deneme
    if (!hata.includes("not found") || Date.now() - kurulumAni < 60000) {
      setTimeout(baglan, YENIDEN_BAGLANMA_MS);
    }
  });
  rozetGuncelle();
}

let kurulumAni = Date.now();

async function portaGonder(mesaj) {
  if (!port) baglan();
  if (!port) {
    bildirim("Çekirdek bağlı değil", "Rust çekirdeği başlatılamadı. Kurulum betiğini çalıştırdınız mı?");
    return false;
  }
  try {
    port.postMessage(mesaj);
    return true;
  } catch {
    port = null;
    portHataMesaji = "Bağlantı koptu";
    bildirim("Çekirdek bağlantısı koptu", "Yerel çekirdek yanıt vermiyor; kurulumu denetleyin.");
    rozetGuncelle();
    return false;
  }
}

// ---------- Olay akışı ----------

async function portMesaji(olay) {
  if (!olay || !olay.status) return;
  switch (olay.status) {
    case "PROGRESS":
    case "STARTED":
    case "COMPLETED":
    case "FAILED":
    case "CANCELED": {
      await isGuncelle(olay);
      rozetGuncelle();
      if (olay.status === "COMPLETED") {
        bildirim("İndirme tamamlandı", `${olay.final_path || "Dosya hazır"}`);
      } else if (olay.status === "FAILED") {
        bildirim("İndirme başarısız", olay.error || "Bilinmeyen hata");
      }
      break;
    }
    case "REFRESH_LINK_REQUIRED":
      await baglantiYenile(olay.download_id);
      break;
    case "REFRESHED":
      await isGuncelle({ ...olay, status: undefined, detail: "Bağlantı yenilendi" });
      break;
  }
}

// ---------- İş kayıtları (storage) ----------

async function isGuncelle(olay) {
  const depo = await chrome.storage.local.get({ isler: {} });
  const isler = depo.isler;
  const id = olay.download_id || "bilinmeyen";
  const kayit = isler[id] || { id, url: "", filename: "" };
  if (olay.status) kayit.status = olay.status;
  if (olay.url) {
    kayit.url = olay.url;
    const baglam = istekBaglami.get(olay.url);
    if (baglam) {
      if (baglam.referer) kayit.referer = baglam.referer;
      if (baglam.origin) kayit.origin = baglam.origin;
      istekBaglami.delete(olay.url);
    }
  }
  if (olay.filename) kayit.filename = olay.filename;
  if (olay.final_path) kayit.finalPath = olay.final_path;
  if (olay.error) kayit.error = olay.error;
  if (Number.isFinite(olay.downloaded_bytes)) kayit.downloaded = olay.downloaded_bytes;
  if (Number.isFinite(olay.total_bytes)) kayit.total = olay.total_bytes;
  if (Number.isFinite(olay.speed_bps)) kayit.speed = olay.speed_bps;
  if (Number.isFinite(olay.eta_sec)) kayit.eta = olay.eta_sec;
  if (olay.detail) kayit.detail = olay.detail;
  kayit.tsGuncelle = Date.now();
  isler[id] = kayit;
  await chrome.storage.local.set({ isler });
}

async function aktifIsSayisi() {
  const depo = await chrome.storage.local.get({ isler: {} });
  return Object.values(depo.isler).filter(
    (i) => i.status === "PROGRESS" || i.status === "STARTED"
  ).length;
}

async function rozetGuncelle() {
  const adet = await aktifIsSayisi();
  await chrome.action.setBadgeText({ text: adet > 0 ? String(adet) : "" });
  await chrome.action.setBadgeBackgroundColor({ color: "#1f6feb" });
}

// ---------- Bağlantı yenileme protokolü ----------

async function baglantiYenile(id) {
  const depo = await chrome.storage.local.get({ isler: {} });
  const is = depo.isler[id];
  if (!is || !is.url) return;
  try {
    const yanit = await fetch(is.url, {
      credentials: "include",
      redirect: "follow",
      cache: "no-store",
    });
    const sonUrl = yanit.url || is.url;
    // Gövdeyi tüketmek yerine akışı hemen kapat
    try {
      yanit.body?.cancel();
    } catch (_) {
      /* akış zaten kapalı olabilir */
    }
    const cerezler = await chrome.cookies.getAll({ url: sonUrl });
    const cerezBasligi = cerezler.map((c) => `${c.name}=${c.value}`).join("; ");
    const basliklar = {
      "User-Agent": navigator.userAgent,
      Accept: "*/*",
    };
    if (is.referer) basliklar.Referer = is.referer;
    if (is.origin) basliklar.Origin = is.origin;
    if (cerezBasligi) basliklar.Cookie = cerezBasligi;
    await portaGonder({
      action: "REFRESH_LINK",
      download_id: id,
      url: sonUrl,
      headers: basliklar,
    });
  } catch {
    bildirim("Bağlantı yenilenemedi", "Kaynak sayfayı yeniden açıp tekrar deneyin.");
  }
}

// ---------- Medya koklayıcı ----------

const gorulen = new Map(); // tabId -> Set<url>
chrome.tabs.onRemoved.addListener((tabId) => {
  gorulen.delete(tabId);
});

function medyaTuru(mime, url) {
  const kucukUrl = url.toLowerCase();
  const m = (mime || "").toLowerCase();
  if (m.includes("mpegurl") || kucukUrl.includes(".m3u8")) return "HLS (.m3u8)";
  if (m.includes("dash+xml") || kucukUrl.includes(".mpd")) return "DASH (.mpd)";
  if (m.startsWith("video/")) return "Video";
  if (m.startsWith("audio/")) return "Ses";
  return null;
}

chrome.webRequest.onHeadersReceived.addListener(
  (detay) => {
    if (detay.tabId < 0) return;
    const basliklar = detay.responseHeaders || [];
    const icerikTuru = basliklar.find((h) => h.name.toLowerCase() === "content-type")?.value || "";
    const tur = medyaTuru(icerikTuru, detay.url);
    if (!tur) return;
    const boyut = Number(basliklar.find((h) => h.name.toLowerCase() === "content-length")?.value || 0);
    if (boyut > 0 && boyut < 512 * 1024 && !tur.startsWith("HLS") && !tur.startsWith("DASH")) return;

    if (!gorulen.has(detay.tabId)) gorulen.set(detay.tabId, new Set());
    const set = gorulen.get(detay.tabId);
    if (set.has(detay.url)) return;
    set.add(detay.url);
    if (set.size > 60) set.delete(set.values().next().value);

    yakalananEkle({
      url: detay.url,
      mime: icerikTuru.split(";")[0],
      tur,
      boyut,
      pageUrl: detay.documentUrl || detay.initiator || "",
      kaynak: "ag",
    });
    chrome.tabs
      .sendMessage(detay.tabId, {
        tip: "INJECT_SNIFFER_BUTTON",
        url: detay.url,
        format: tur,
        filename: dosyaAdi(detay.url, basliklar),
      })
      .catch(() => {});
  },
  { urls: ["<all_urls>"] },
  ["responseHeaders"]
);

function dosyaAdi(url, basliklar) {
  const yerlesim = basliklar.find((h) => h.name.toLowerCase() === "content-disposition")?.value;
  if (yerlesim) {
    const es = yerlesim.match(/filename\*?=(?:UTF-8''|")?([^";]+)/i);
    if (es && es[1]) {
      try {
        return decodeURIComponent(es[1].replace(/["']/g, ""));
      } catch (_) {
        return es[1].replace(/["']/g, "");
      }
    }
  }
  try {
    const u = new URL(url);
    const son = u.pathname.split("/").filter(Boolean).pop();
    return son ? decodeURIComponent(son) : "";
  } catch (_) {
    return "";
  }
}

async function yakalananEkle(kayit) {
  const depo = await chrome.storage.local.get({ yakalanan: [] });
  const liste = depo.yakalanan;
  const eskiIndex = liste.findIndex((k) => k.url === kayit.url);
  kayit.ts = Date.now();
  if (eskiIndex >= 0) liste[eskiIndex] = kayit;
  else {
    liste.unshift(kayit);
    if (liste.length > YAKALANAN_SINIR) liste.length = YAKALANAN_SINIR;
  }
  await chrome.storage.local.set({ yakalanan: liste });
}

// MSE köprüsünden gelen birikimli metaveri
const mseToplam = new Map(); // pageUrl -> {bytes, mime}

// ---------- İndirme başlatma ----------

async function indirBaslat({ url, filename, pageUrl }) {
  if (!/^https?:\/\//i.test(url)) {
    bildirim("Desteklenmeyen adres", "Yalnızca http(s) adresleri indirilebilir.");
    return;
  }
  const ayarlar = (await chrome.storage.local.get({ ayarlar: { eslik: 16 } })).ayarlar;
  const hedef = pageUrl || "";
  let cerezler = [];
  try {
    cerezler = await chrome.cookies.getAll({ url });
  } catch (_) {
    /* çerez izni yok sayılır */
  }
  const basliklar = {
    "User-Agent": navigator.userAgent,
    Accept: "*/*",
  };
  if (hedef) {
    basliklar.Referer = hedef;
    try {
      basliklar.Origin = new URL(hedef).origin;
    } catch (_) {
      /* geçersiz sayfa adresi */
    }
  }
  const cerezBasligi = cerezler.map((c) => `${c.name}=${c.value}`).join("; ");
  if (cerezBasligi) basliklar.Cookie = cerezBasligi;
  if (istekBaglami.size >= 100) istekBaglami.delete(istekBaglami.keys().next().value);
  istekBaglami.set(url, { referer: basliklar.Referer, origin: basliklar.Origin });

  await portaGonder({
    action: "START_DOWNLOAD",
    url,
    filename: filename || undefined,
    headers: basliklar,
    concurrency: ayarlar.eslik,
  });
}

// ---------- Mesaj merkezi ----------

chrome.runtime.onMessage.addListener((mesaj, gonderen, yanitla) => {
  (async () => {
    switch (mesaj?.tip) {
      case "DOM_MEDIA_DISCOVERED": {
        const p = mesaj.payload || {};
        if (p.type === "SEGMENT_APPENDED" && mesaj.pageUrl) {
          const anlik =
            mseToplam.get(mesaj.pageUrl) ||
            { bytes: 0, mime: p.mimeType || "video/mp4", bildirildi: false };
          if (Number.isFinite(p.toplamBayt)) {
            anlik.bytes = Math.max(anlik.bytes, p.toplamBayt);
          } else {
            anlik.bytes += p.byteLength || 0;
          }
          if (p.mimeType) anlik.mime = p.mimeType;
          if (!mseToplam.has(mesaj.pageUrl) && mseToplam.size >= 200) {
            mseToplam.delete(mseToplam.keys().next().value);
          }
          mseToplam.set(mesaj.pageUrl, anlik);
          if (!anlik.bildirildi && anlik.bytes >= 2 * 1024 * 1024) {
            anlik.bildirildi = true;
            await yakalananEkle({
              url: `mse:${mesaj.pageUrl}`,
              mime: anlik.mime,
              tur: "MSE Akışı",
              boyut: anlik.bytes,
              pageUrl: mesaj.pageUrl,
              baslik: mesaj.title,
              kaynak: "mse",
            });
          }
        } else if (p.type === "MSE_BLOB_CREATED" && mesaj.pageUrl) {
          await yakalananEkle({
            url: `mse:${mesaj.pageUrl}`,
            mime: "video/mp4",
            tur: "MSE Akışı",
            boyut: 0,
            pageUrl: mesaj.pageUrl,
            baslik: mesaj.title,
            kaynak: "mse",
          });
        }
        yanitla({ ok: true });
        break;
      }
      case "USER_REQUESTED_DOWNLOAD": {
        await indirBaslat(mesaj);
        yanitla({ ok: true });
        break;
      }
      case "POPUP_DURUM": {
        const depo = await chrome.storage.local.get({
          isler: {},
          yakalanan: [],
          ayarlar: { eslik: 16 },
        });
        yanitla({
          bagli: !!port,
          portHata: portHataMesaji,
          ...depo,
        });
        break;
      }
      case "KOMUT": {
        const { komut, download_id } = mesaj;
        const eylem = { DURAKLAT: "PAUSE", DEVAM: "RESUME", IPTAL: "CANCEL" }[komut];
        if (eylem) {
          await portaGonder({ action: eylem, download_id });
          if (komut === "DURAKLAT") {
            await isGuncelle({ status: "duraklatildi", download_id, detail: "duraklatıldı" });
          } else if (komut === "DEVAM") {
            await isGuncelle({ status: "PROGRESS", download_id, detail: "devam ediyor" });
          }
          yanitla({ ok: true });
        } else if (komut === "YAKALANAN_SIL") {
          await chrome.storage.local.set({ yakalanan: [] });
          yanitla({ ok: true });
        } else if (komut === "IS_SIL") {
          const depo = await chrome.storage.local.get({ isler: {} });
          const kayit = depo.isler[download_id];
          if (
            kayit &&
            (kayit.status === "PROGRESS" || kayit.status === "STARTED" || kayit.status === "duraklatildi")
          ) {
            await portaGonder({ action: "CANCEL", download_id });
          }
          delete depo.isler[download_id];
          await chrome.storage.local.set({ isler: depo.isler });
          rozetGuncelle();
          yanitla({ ok: true });
        }
        break;
      }
      case "AYAR_KAYDET": {
        const ayarlar = { eslik: Number(mesaj.eslik) || 16 };
        await chrome.storage.local.set({ ayarlar });
        yanitla({ ok: true });
        break;
      }
      default:
        yanitla({ ok: false });
    }
  })();
  return true; // asenkron yanıt
});

// ---------- Bağlam menüsü ----------

chrome.runtime.onInstalled.addListener(() => {
  chrome.contextMenus.create({
    id: "titan-indir",
    title: "Titanium ile indir",
    contexts: ["link", "video", "audio"],
  });
  kurulumAni = Date.now();
  baglan();
});

chrome.contextMenus.onClicked.addListener((bilgi) => {
  if (bilgi.menuItemId !== "titan-indir") return;
  const url = bilgi.linkUrl || bilgi.srcUrl;
  if (!url) return;
  indirBaslat({ url, filename: "", pageUrl: bilgi.pageUrl || "" });
});

// ---------- Bekçi alarmı (Chrome 105+ önerisi) ----------

chrome.alarms.onAlarm.addListener((alarm) => {
  if (alarm.name === "titan-bekci") baglan();
});

chrome.alarms.create("titan-bekci", { periodInMinutes: BEKCI_PERIYOT_DAKIKA });

// ---------- Yardımcılar ----------

function bildirim(baslik, metin) {
  chrome.notifications.create({
    type: "basic",
    iconUrl: chrome.runtime.getURL("icons/128.png"),
    title: baslik,
    message: metin || "",
  });
}

// Hizmet işçisi her uyandığında bağlantıyı tazele
baglan();
