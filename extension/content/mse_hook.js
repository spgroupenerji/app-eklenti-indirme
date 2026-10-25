// Titanium MSE Kancası — sayfanın ana JavaScript dünyasında çalışır (world: MAIN)
(function () {
  "use strict";
  if (window.__TITAN_KURULDU) return;
  window.__TITAN_KURULDU = true;

  const RAPOR_ARALIK_BAYT = 4 * 1024 * 1024;
  const guvenli = (fn) => {
    try {
      return fn();
    } catch (_) {
      return undefined;
    }
  };

  const orijinalCreateObjectURL = URL.createObjectURL;
  const orijinalAddSourceBuffer = window.MediaSource?.prototype?.addSourceBuffer;
  const orijinalAppendBuffer = window.SourceBuffer?.prototype?.appendBuffer;

  function bildir(yuk) {
    guvenli(() =>
      window.postMessage(Object.assign({ source: "TITAN_SNIFFER" }, yuk), "*")
    );
  }

  if (orijinalCreateObjectURL) {
    URL.createObjectURL = function (hedef) {
      const url = orijinalCreateObjectURL.apply(this, arguments);
      if (hedef instanceof MediaSource) {
        bildir({ type: "MSE_BLOB_CREATED" });
      }
      return url;
    };
  }

  if (orijinalAddSourceBuffer) {
    MediaSource.prototype.addSourceBuffer = function (mimeTuru) {
      const tampon = orijinalAddSourceBuffer.apply(this, arguments);
      guvenli(() => {
        Object.defineProperty(tampon, "_titanMime", {
          value: mimeTuru,
          configurable: true,
          enumerable: false,
        });
      });
      return tampon;
    };
  }

  if (orijinalAppendBuffer) {
    const sayac = new WeakMap();
    SourceBuffer.prototype.appendBuffer = function (veri) {
      if (veri && veri.byteLength > 0) {
        const oAnki = (sayac.get(this) || 0) + veri.byteLength;
        sayac.set(this, oAnki);
        const sonRapor = this.__titanSonRapor || 0;
        if (oAnki - sonRapor >= RAPOR_ARALIK_BAYT || oAnki === veri.byteLength) {
          this.__titanSonRapor = oAnki;
          bildir({
            type: "SEGMENT_APPENDED",
            byteLength: veri.byteLength,
            toplamBayt: oAnki,
            mimeType: this._titanMime,
          });
        }
      }
      return orijinalAppendBuffer.apply(this, arguments);
    };
  }
})();
