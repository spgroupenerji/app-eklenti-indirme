use crate::protocol::Command;
use std::io::{Read, Write};
use tokio::sync::mpsc;

const MAX_MESSAGE: usize = 16 * 1024 * 1024;

pub struct Sender {
    cikis: std::sync::Mutex<std::io::Stdout>,
}

impl Sender {
    pub fn new() -> Self {
        Self {
            cikis: std::sync::Mutex::new(std::io::stdout()),
        }
    }
    pub fn gonder(&self, json: &[u8]) {
        let mut out = self.cikis.lock().unwrap();
        let _ = out.write_all(&(json.len() as u32).to_le_bytes());
        let _ = out.write_all(json);
        let _ = out.flush();
    }
}

pub fn stdin_okuyucu(kanal: mpsc::UnboundedSender<Command>) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut stdin = std::io::stdin().lock();
        loop {
            let mut boyut_buf = [0u8; 4];
            if stdin.read_exact(&mut boyut_buf).is_err() {
                break;
            }
            let boyut = u32::from_le_bytes(boyut_buf) as usize;
            if boyut == 0 || boyut > MAX_MESSAGE {
                break;
            }
            let mut buf = vec![0u8; boyut];
            if stdin.read_exact(&mut buf).is_err() {
                break;
            }
            match serde_json::from_slice::<Command>(&buf) {
                Ok(komut) => {
                    if kanal.send(komut).is_err() {
                        break;
                    }
                }
                Err(_) => {
                    // Hatalı komut sessizce yoksayılır; kanal açık kalır
                }
            }
        }
        let _ = kanal.send(Command::Shutdown);
    })
}
