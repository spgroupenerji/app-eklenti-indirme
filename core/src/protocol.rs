use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Deserialize, Debug)]
#[serde(tag = "action")]
pub enum Command {
    #[serde(rename = "START_DOWNLOAD")]
    Start {
        url: String,
        #[serde(default)]
        filename: Option<String>,
        #[serde(default)]
        headers: HashMap<String, String>,
        #[serde(default)]
        concurrency: Option<u32>,
    },
    #[serde(rename = "CANCEL")]
    Cancel { download_id: String },
    #[serde(rename = "PAUSE")]
    Pause { download_id: String },
    #[serde(rename = "RESUME")]
    Resume { download_id: String },
    #[serde(rename = "REFRESH_LINK")]
    RefreshLink {
        download_id: String,
        url: String,
        #[serde(default)]
        headers: HashMap<String, String>,
    },
    #[serde(rename = "LIST")]
    List,
    #[serde(rename = "PING")]
    Ping,
    #[serde(rename = "SHUTDOWN")]
    Shutdown,
}

#[derive(Serialize, Debug, Clone)]
pub struct Event {
    pub status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub download_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub downloaded_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed_bps: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eta_sec: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub final_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jobs: Option<Vec<JobSnapshot>>,
}

#[derive(Serialize, Debug, Clone)]
pub struct JobSnapshot {
    pub download_id: String,
    pub url: String,
    pub filename: String,
    pub state: String,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub speed_bps: f64,
}

impl Event {
    fn base(status: &'static str) -> Self {
        Self {
            status,
            download_id: None,
            url: None,
            downloaded_bytes: None,
            total_bytes: None,
            speed_bps: None,
            eta_sec: None,
            final_path: None,
            filename: None,
            detail: None,
            error: None,
            jobs: None,
        }
    }
    pub fn pong() -> Self {
        Self::base("PONG")
    }
    pub fn started(id: &str, url: &str, filename: &str) -> Self {
        Self {
            url: Some(url.to_string()),
            filename: Some(filename.to_string()),
            download_id: Some(id.to_string()),
            ..Self::base("STARTED")
        }
    }
    pub fn progress(id: &str, indirilen: u64, toplam: u64, hiz: f64, detay: Option<String>) -> Self {
        let eta = if hiz > 1.0 && toplam > indirilen {
            Some((toplam - indirilen) as f64 / hiz)
        } else {
            None
        };
        Self {
            download_id: Some(id.to_string()),
            downloaded_bytes: Some(indirilen),
            total_bytes: Some(toplam),
            speed_bps: Some(hiz),
            eta_sec: eta,
            detail: detay,
            ..Self::base("PROGRESS")
        }
    }
    pub fn completed(id: &str, yol: &str) -> Self {
        Self {
            download_id: Some(id.to_string()),
            final_path: Some(yol.to_string()),
            ..Self::base("COMPLETED")
        }
    }
    pub fn canceled(id: &str) -> Self {
        Self {
            download_id: Some(id.to_string()),
            ..Self::base("CANCELED")
        }
    }
    pub fn refresh_required(id: &str) -> Self {
        Self {
            download_id: Some(id.to_string()),
            ..Self::base("REFRESH_LINK_REQUIRED")
        }
    }
    pub fn refreshed(id: &str) -> Self {
        Self {
            download_id: Some(id.to_string()),
            ..Self::base("REFRESHED")
        }
    }
    pub fn failed(id: &str, hata: &str) -> Self {
        Self {
            download_id: Some(id.to_string()),
            error: Some(hata.to_string()),
            ..Self::base("FAILED")
        }
    }
    pub fn list(jobs: Vec<JobSnapshot>) -> Self {
        Self {
            jobs: Some(jobs),
            ..Self::base("LIST_RESULT")
        }
    }
}
