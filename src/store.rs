use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::util::{now_rfc3339, short_title};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Stats {
    pub tokens: u64,
    pub tokens_per_sec: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
    #[serde(default)]
    pub thinking: String,
    #[serde(default)]
    pub thinking_open: bool,
    #[serde(default)]
    pub stats: Option<Stats>,
}

impl Message {
    pub fn user(content: String) -> Self {
        Self {
            role: "user".into(),
            content,
            thinking: String::new(),
            thinking_open: false,
            stats: None,
        }
    }

    pub fn assistant() -> Self {
        Self {
            role: "assistant".into(),
            content: String::new(),
            thinking: String::new(),
            thinking_open: false,
            stats: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Conversation {
    pub id: String,
    pub title: String,
    pub model: String,
    #[serde(default)]
    pub system: String,
    #[serde(default)]
    pub messages: Vec<Message>,
    pub updated_at: String,
    #[serde(skip)]
    pub persisted: bool,
    #[serde(skip)]
    pub draft: String,
}

impl Conversation {
    pub fn new(model: String) -> Self {
        Self {
            id: format!("c{}", chrono::Utc::now().timestamp_millis()),
            title: "New chat".into(),
            model,
            system: String::new(),
            messages: Vec::new(),
            updated_at: now_rfc3339(),
            persisted: false,
            draft: String::new(),
        }
    }

    pub fn touch_title(&mut self) {
        if self.title != "New chat" {
            return;
        }
        if let Some(first) = self.messages.iter().find(|m| m.role == "user") {
            self.title = short_title(&first.content);
        }
    }
}

pub fn data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("ollatui")
}

pub fn chats_dir() -> PathBuf {
    data_dir().join("chats")
}

pub fn load_all() -> Vec<Conversation> {
    let dir = chats_dir();
    let mut chats = Vec::new();
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(_) => return chats,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(mut chat) = serde_json::from_str::<Conversation>(&text) else {
            continue;
        };
        chat.persisted = true;
        chats.push(chat);
    }
    chats.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    chats
}

pub fn save(chat: &Conversation) -> anyhow::Result<()> {
    let dir = chats_dir();
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{}.json", chat.id));
    let text = serde_json::to_string_pretty(chat)?;
    std::fs::write(path, text)?;
    Ok(())
}

pub fn delete(id: &str) -> anyhow::Result<()> {
    let path = chats_dir().join(format!("{id}.json"));
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    Ok(())
}
