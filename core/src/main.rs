#![cfg_attr(windows, windows_subsystem = "windows")]

mod dash;
mod engine;
mod hls;
mod job;
mod nm;
mod probe;
mod protocol;
mod remux;
mod storage;
mod util;

use protocol::{Command, Event};
use std::sync::Arc;
use tokio::sync::mpsc;

#[tokio::main]
async fn main() {
    let (olay_tx, mut olay_rx) = mpsc::unbounded_channel::<Event>();
    let gonderen = Arc::new(nm::Sender::new());

    {
        let gonderen = gonderen.clone();
        tokio::spawn(async move {
            while let Some(olay) = olay_rx.recv().await {
                if let Ok(json) = serde_json::to_vec(&olay) {
                    gonderen.gonder(&json);
                }
            }
        });
    }

    let (komut_tx, mut komut_rx) = mpsc::unbounded_channel::<Command>();
    nm::stdin_okuyucu(komut_tx);

    let uygulama = job::Uygulama::yeni(olay_tx.clone());

    while let Some(komut) = komut_rx.recv().await {
        match komut {
            Command::Ping => {
                let _ = olay_tx.send(Event::pong());
            }
            Command::Shutdown => break,
            Command::List => {
                let _ = olay_tx.send(Event::list(uygulama.anlik_liste()));
            }
            Command::Start {
                url,
                filename,
                headers,
                concurrency,
            } => {
                let id = uygulama.baslat(url.clone(), filename.clone(), headers, concurrency);
                let ad = filename.unwrap_or_default();
                let _ = olay_tx.send(Event::started(&id, &url, &ad));
            }
            Command::Cancel { download_id } => uygulama.komut(Command::Cancel { download_id }),
            Command::Pause { download_id } => uygulama.komut(Command::Pause { download_id }),
            Command::Resume { download_id } => uygulama.komut(Command::Resume { download_id }),
            Command::RefreshLink {
                download_id,
                url,
                headers,
            } => {
                uygulama.komut(Command::RefreshLink {
                    download_id,
                    url,
                    headers,
                })
            }
        }
    }
}
