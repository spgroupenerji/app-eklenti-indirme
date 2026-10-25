// Titanium Popup — durum gösterimi ve komut gönderimi
"use strict";

const $ = (id) => document.getElementById(id);

function dosyaAdiCoz(url) {
  const ham = url.split("/").pop()?.split("?")[0] || "";
  try {
    return decodeURIComponent(ham);
  } catch (_) {
    return ham;
  }
}

function baytBiçim(n) {
  if (!Number.isFinite(n) || n <= 0) return "—";
  const birim = ["B", "KB", "MB", "GB", "TB"];
  let i = 0;
  let v = n;
  while (v >= 1024 && i < birim.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v.toFixed(v >= 100 || i === 0 ? 0 : 1)} ${birim[i]}`;
}

function hizBiçim(bps) {
  if (!Number.isFinite(bps) || bps <= 0) return "—";
  return `${baytBiçim(bps)}/s`;
}

function sureBiçim(sn) {
  if (!Number.isFinite(sn) || sn <= 0) return "—";
  const s = Math.round(sn);
  if (s < 60) return `${s} sn`;
  const d = Math.floor(s / 60);
  if (d < 60) return `${d} dk`;
  return `${Math.floor(d / 60)} sa ${d % 60} dk`;
}

function durumMetni(status) {
  switch (status) {
    case "PROGRESS": return "İndiriliyor";
    case "STARTED": return "Başlatılıyor";
    case "COMPLETED": return "Tamamlandı";
    case "FAILED": return "Başarısız";
    case "CANCELED": return "İptal edildi";
    case "duraklatildi": return "Duraklatıldı";
    default: return status || "";
  }
}

function isKartiCiz(is, sablon) {
  const dugum = sablon.content.firstElementChild.cloneNode(true);
  const ad = dugum.querySelector(".is-ad");
  ad.textContent = is.filename || is.id;
  ad.title = is.finalPath || is.url || "";
  const durum = dugum.querySelector(".is-durum");
  durum.textContent = is.error ? `${durumMetni(is.status)}: ${is.error}` : durumMetni(is.status);
  durum.classList.add(
    is.status === "COMPLETED" ? "tamamlandi" :
    is.status === "FAILED" ? "hata" :
    is.status === "CANCELED" ? "iptal" : ""
  );

  const yuzde =
    is.total > 0 ? Math.min(100, Math.round((is.downloaded / is.total) * 100)) : 0;
  dugum.querySelector(".dolgu").style.width = `${yuzde}%`;

  const parcalar = [];
  if (is.total > 0) parcalar.push(`${baytBiçim(is.downloaded)} / ${baytBiçim(is.total)}`);
  else if (is.downloaded > 0) parcalar.push(baytBiçim(is.downloaded));
  if (is.speed > 0) parcalar.push(hizBiçim(is.speed));
  if (is.eta > 0 && is.status === "PROGRESS") parcalar.push(`kalan ${sureBiçim(is.eta)}`);
  if (is.detail && is.status === "PROGRESS") parcalar.push(is.detail);
  dugum.querySelector(".is-detay").textContent = parcalar.join(" · ");

  const calisiyor = is.status === "PROGRESS" || is.status === "STARTED";
  const duraklatildi = is.status === "duraklatildi";
  dugum.querySelector(".k-duraklat").classList.toggle("gizli", !calisiyor);
  dugum.querySelector(".k-devam").classList.toggle("gizli", !duraklatildi);
  dugum.querySelector(".k-iptal").classList.toggle("gizli", !(calisiyor || duraklatildi));

  dugum.querySelector(".k-duraklat").addEventListener("click", () =>
    komutGonder("DURAKLAT", is.id));
  dugum.querySelector(".k-devam").addEventListener("click", () =>
    komutGonder("DEVAM", is.id));
  dugum.querySelector(".k-iptal").addEventListener("click", () =>
    komutGonder("IPTAL", is.id));
  dugum.querySelector(".k-sil").classList.toggle("gizli", calisiyor);
  dugum.querySelector(".k-sil").addEventListener("click", () => komutGonder("IS_SIL", is.id));
  return dugum;
}

function yakalananKartiCiz(kayit, sablon) {
  const dugum = sablon.content.firstElementChild.cloneNode(true);
  dugum.querySelector(".yak-tur").textContent = kayit.tur || "Medya";
  const ad = dugum.querySelector(".yak-ad");
  ad.textContent = kayit.kaynak === "mse"
    ? kayit.baslik || "MediaSource akışı"
    : kayit.url.split("/").pop()?.split("?")[0] || kayit.url;
  ad.title = kayit.url;
  dugum.querySelector(".yak-meta").textContent = [
    kayit.mime,
    kayit.boyut > 0 ? baytBiçim(kayit.boyut) : null,
  ].filter(Boolean).join(" · ");

  const dugme = dugum.querySelector(".yak-indir");
  if (kayit.kaynak === "mse") {
    dugme.disabled = true;
    dugme.title = "MSE akışları ağ adresi içermez; sayfadaki indirme düğmesini kullanın";
    dugme.textContent = "Akış";
  } else {
    dugme.addEventListener("click", () => {
      chrome.runtime.sendMessage({
        tip: "USER_REQUESTED_DOWNLOAD",
        url: kayit.url,
        filename: dosyaAdiCoz(kayit.url),
        pageUrl: kayit.pageUrl,
      });
      dugme.textContent = "Başladı";
      dugme.disabled = true;
    });
  }
  return dugum;
}

function ciz(depo) {
  const isler = Object.values(depo.isler || {}).sort(
    (a, b) => (b.tsGuncelle || 0) - (a.tsGuncelle || 0)
  );
  const isSablon = $("isSablon");
  const liste = $("isListesi");
  liste.replaceChildren(...isler.map((i) => isKartiCiz(i, isSablon)));
  $("isBos").classList.toggle("gizli", isler.length > 0);

  const yakalanan = depo.yakalanan || [];
  const yakSablon = $("yakalananSablon");
  const yakListe = $("yakalananListesi");
  yakListe.replaceChildren(...yakalanan.slice(0, 20).map((k) => yakalananKartiCiz(k, yakSablon)));
  $("yakalananBos").classList.toggle("gizli", yakalanan.length > 0);
  $("yakalananTemizle").classList.toggle("gizli", yakalanan.length === 0);
}

function komutGonder(komut, download_id) {
  chrome.runtime.sendMessage({ tip: "KOMUT", komut, download_id });
}

async function yukle() {
  try {
    const depo = await chrome.runtime.sendMessage({ tip: "POPUP_DURUM" });
    if (depo) {
      $("durumNokta").classList.toggle("bagli", !!depo.bagli);
      $("durumMetin").textContent = depo.bagli
        ? "Çekirdek bağlı"
        : "Çekirdek bağlı değil — kurulum betiğini çalıştırın";
      if (depo.ayarlar?.eslik) $("eslikSec").value = String(depo.ayarlar.eslik);
      ciz(depo);
    }
  } catch (_) {
    $("durumMetin").textContent = "Arka plan yanıt vermiyor";
  }
}

chrome.storage.onChanged.addListener((degisiklik, alan) => {
  if (alan !== "local") return;
  chrome.storage.local.get({ isler: {}, yakalanan: [] }).then(ciz);
});

$("eslikSec").addEventListener("change", (e) => {
  chrome.runtime.sendMessage({ tip: "AYAR_KAYDET", eslik: Number(e.target.value) });
});

$("yakalananTemizle").addEventListener("click", () => komutGonder("YAKALANAN_SIL"));

yukle();
