// Titanium Köprüsü — izole dünyada çalışır; MAIN dünyası ile Service Worker arasında taşır
(function () {
  "use strict";

  window.addEventListener("message", (olay) => {
    if (olay.source !== window) return;
    const veri = olay.data;
    if (!veri || veri.source !== "TITAN_SNIFFER") return;
    chrome.runtime.sendMessage({
      tip: "DOM_MEDIA_DISCOVERED",
      payload: veri,
      pageUrl: location.href,
      title: document.title,
    });
  });

  chrome.runtime.onMessage.addListener((mesaj) => {
    if (mesaj?.tip !== "INJECT_SNIFFER_BUTTON" || !document.body) return;
    let dugme = document.getElementById("titan-sniff-btn");
    if (!dugme) {
      dugme = document.createElement("button");
      dugme.id = "titan-sniff-btn";
      dugme.setAttribute(
        "style",
        [
          "position:fixed",
          "top:15px",
          "right:15px",
          "z-index:2147483647",
          "background:#0d1117",
          "color:#58a6ff",
          "border:1px solid #30363d",
          "padding:8px 12px",
          "border-radius:6px",
          "font-family:system-ui,sans-serif",
          "font-size:12px",
          "cursor:pointer",
          "box-shadow:0 8px 24px rgba(0,0,0,.5)",
        ].join(";")
      );
      dugme.addEventListener("click", () => {
        chrome.runtime.sendMessage({
          tip: "USER_REQUESTED_DOWNLOAD",
          url: dugme.dataset.url,
          filename: dugme.dataset.filename,
          pageUrl: location.href,
        });
        dugme.textContent = "Aktarılıyor…";
        dugme.disabled = true;
      });
      document.body.appendChild(dugme);
    }
    dugme.dataset.url = mesaj.url;
    dugme.dataset.filename = mesaj.filename || "";
    dugme.disabled = false;
    dugme.textContent = `⬇ Medyayı İndir [${mesaj.format}]`;
    dugme.style.display = "";

    clearTimeout(window.__titanButonZamanlayici);
    window.__titanButonZamanlayici = setTimeout(() => {
      const b = document.getElementById("titan-sniff-btn");
      if (b) b.style.display = "none";
    }, 45000);
  });
})();
