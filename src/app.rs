use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use tokio::sync::mpsc::UnboundedSender;
use tokio::task::JoinHandle;

use crate::config::{self, Config};
use crate::field::Field;
use crate::hf::{self, GgufFile};
use crate::ollama::{self, ChatEvent, ChatParams, ChatTurn, Installed, ModelDetail, RunningModel};
use crate::store::{self, Conversation, Message};
use crate::theme::{self, Theme};
use crate::util::now_rfc3339;

pub enum Bus {
    Probe(bool),
    Tags(Vec<Installed>),
    Running(Vec<RunningModel>),
    Show {
        name: String,
        detail: ModelDetail,
    },
    ChatDelta {
        id: u64,
        content: String,
        thinking: String,
    },
    ChatDone {
        id: u64,
        tokens: u64,
        tokens_per_sec: f64,
    },
    ChatErr {
        id: u64,
        message: String,
    },
    ChatIdle {
        id: u64,
    },
    PullTick {
        id: u64,
        status: String,
        completed: u64,
        total: u64,
    },
    PullDone {
        id: u64,
    },
    PullErr {
        id: u64,
        message: String,
    },
    SearchOk {
        id: u64,
        models: Vec<hf::Model>,
    },
    SearchErr {
        id: u64,
        message: String,
    },
    FilesOk {
        id: u64,
        repo: String,
        files: Vec<GgufFile>,
    },
    FilesErr {
        id: u64,
        message: String,
    },
    Notice {
        error: bool,
        message: String,
        refresh: bool,
    },
    Service(String),
    OmarchyChanged,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Chat,
    Models,
    Marketplace,
    Settings,
}

impl Screen {
    pub const ALL: [Screen; 4] = [
        Screen::Chat,
        Screen::Models,
        Screen::Marketplace,
        Screen::Settings,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Screen::Chat => "Chat",
            Screen::Models => "Models",
            Screen::Marketplace => "Marketplace",
            Screen::Settings => "Settings",
        }
    }

    fn index(self) -> usize {
        Self::ALL.iter().position(|s| *s == self).unwrap_or(0)
    }

    fn next(self) -> Self {
        Self::ALL[(self.index() + 1) % Self::ALL.len()]
    }

    fn prev(self) -> Self {
        let i = self.index();
        Self::ALL[if i == 0 { Self::ALL.len() - 1 } else { i - 1 }]
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ChatPane {
    Sidebar,
    Transcript,
    Composer,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ModelsPane {
    List,
    Pull,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum MarketPane {
    Search,
    Results,
    Files,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Setting {
    Autostart,
    StopOnQuit,
    Theme,
    Host,
    DefaultModel,
    AssistantName,
    Temperature,
    TopP,
    NumCtx,
    Seed,
    KeepAlive,
    TokenStats,
    Insecure,
    OpenConfig,
}

impl Setting {
    pub const ALL: [Setting; 14] = [
        Setting::Autostart,
        Setting::StopOnQuit,
        Setting::Theme,
        Setting::Host,
        Setting::DefaultModel,
        Setting::AssistantName,
        Setting::Temperature,
        Setting::TopP,
        Setting::NumCtx,
        Setting::Seed,
        Setting::KeepAlive,
        Setting::TokenStats,
        Setting::Insecure,
        Setting::OpenConfig,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Setting::Autostart => "Autostart Ollama",
            Setting::StopOnQuit => "Stop on quit",
            Setting::Theme => "Theme",
            Setting::Host => "Host",
            Setting::DefaultModel => "Default model",
            Setting::AssistantName => "Assistant name",
            Setting::Temperature => "Temperature",
            Setting::TopP => "Top P",
            Setting::NumCtx => "Context",
            Setting::Seed => "Seed",
            Setting::KeepAlive => "Keep alive",
            Setting::TokenStats => "Token stats",
            Setting::Insecure => "Insecure HF pulls",
            Setting::OpenConfig => "Open config folder",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Setting::Autostart => "Starts Ollama when this app opens",
            Setting::StopOnQuit => "Stops Ollama when this app quits",
            Setting::Theme => "Enter opens the palette list",
            Setting::Host => "Ollama server URL",
            Setting::DefaultModel => "Model used for a new chat",
            Setting::AssistantName => "Name shown above replies",
            Setting::Temperature => "Left and right nudge by 0.1",
            Setting::TopP => "Left and right nudge by 0.05",
            Setting::NumCtx => "Left and right nudge by 1024",
            Setting::Seed => "Blank uses a random seed",
            Setting::KeepAlive => "0, 5m, 30m, 1h, or -1 to keep loaded",
            Setting::TokenStats => "Tokens and speed after each reply",
            Setting::Insecure => "Lets Ollama follow Hugging Face redirects",
            Setting::OpenConfig => "Opens the folder in the file manager",
        }
    }
}

#[derive(Clone)]
pub struct PullView {
    pub label: String,
    pub status: String,
    pub completed: u64,
    pub total: u64,
}

pub enum Confirm {
    DeleteChat { id: String, title: String },
    DeleteModel(String),
}

pub struct App {
    pub config: Config,
    config_path: PathBuf,
    pub theme: Theme,
    pub omarchy: Option<Theme>,
    pub omarchy_name: String,
    watch: Option<PathBuf>,
    pub screen: Screen,
    pub help: bool,
    pub confirm: Option<Confirm>,
    pub status: String,
    pub status_is_error: bool,
    pub connected: Option<bool>,
    pub service: String,
    service_action: Option<&'static str>,
    quit_requested: bool,
    pub chats: Vec<Conversation>,
    pub chat_index: usize,
    pub chat_pane: ChatPane,
    pub show_chats: bool,
    pub composer: Field,
    pub slash_pick: usize,
    pub stick_bottom: bool,
    pub scroll: usize,
    pub transcript_len: usize,
    pub transcript_view: usize,
    pub transcript_cursor: usize,
    pub streaming: bool,
    pub streaming_chat: Option<String>,
    pub model_picker: bool,
    pub model_filter: Field,
    pub model_pick: usize,
    pub renaming: bool,
    pub rename_field: Field,
    pub editing_system: bool,
    pub system_field: Field,
    pub models: Vec<Installed>,
    pub running: Vec<RunningModel>,
    pub model_index: usize,
    pub models_pane: ModelsPane,
    pub pull_field: Field,
    pub detail: Option<ModelDetail>,
    shown_for: String,
    pub hf_query: Field,
    pub hf_sort: hf::Sort,
    pub hf_results: Vec<hf::Model>,
    pub hf_index: usize,
    pub hf_pane: MarketPane,
    pub hf_files: Vec<GgufFile>,
    pub hf_file_index: usize,
    pub hf_repo: String,
    pub hf_loading: bool,
    pub settings_index: usize,
    pub picking_theme: bool,
    pub theme_index: usize,
    pub editing: bool,
    pub edit_field: Field,
    pub pull: Option<PullView>,
    events: UnboundedSender<Bus>,
    chat_job: u64,
    pull_job: u64,
    search_job: u64,
    files_job: u64,
    chat_task: Option<JoinHandle<()>>,
    pull_task: Option<JoinHandle<()>>,
    search_task: Option<JoinHandle<()>>,
    files_task: Option<JoinHandle<()>>,
    watcher_started: bool,
}

impl App {
    pub fn new(
        config: Config,
        config_path: PathBuf,
        mut chats: Vec<Conversation>,
        omarchy: Option<Theme>,
        omarchy_name: String,
        watch: Option<PathBuf>,
        events: UnboundedSender<Bus>,
    ) -> Self {
        if chats.is_empty() {
            chats.push(Conversation::new(config.default_model.clone()));
        }
        let theme = theme::resolve(&config.theme, omarchy.as_ref());
        let mut app = Self {
            config,
            config_path,
            theme,
            omarchy,
            omarchy_name,
            watch,
            screen: Screen::Chat,
            help: false,
            confirm: None,
            status: String::new(),
            status_is_error: false,
            connected: None,
            service: "…".into(),
            service_action: None,
            quit_requested: false,
            chats,
            chat_index: 0,
            chat_pane: ChatPane::Composer,
            show_chats: true,
            composer: Field::default(),
            slash_pick: 0,
            stick_bottom: true,
            scroll: 0,
            transcript_len: 0,
            transcript_view: 8,
            transcript_cursor: 0,
            streaming: false,
            streaming_chat: None,
            model_picker: false,
            model_filter: Field::default(),
            model_pick: 0,
            renaming: false,
            rename_field: Field::default(),
            editing_system: false,
            system_field: Field::default(),
            models: Vec::new(),
            running: Vec::new(),
            model_index: 0,
            models_pane: ModelsPane::List,
            pull_field: Field::default(),
            detail: None,
            shown_for: String::new(),
            hf_query: Field::default(),
            hf_sort: hf::Sort::Downloads,
            hf_results: Vec::new(),
            hf_index: 0,
            hf_pane: MarketPane::Search,
            hf_files: Vec::new(),
            hf_file_index: 0,
            hf_repo: String::new(),
            hf_loading: false,
            settings_index: 0,
            picking_theme: false,
            theme_index: 0,
            editing: false,
            edit_field: Field::default(),
            pull: None,
            events,
            chat_job: 0,
            pull_job: 0,
            search_job: 0,
            files_job: 0,
            chat_task: None,
            pull_task: None,
            search_task: None,
            files_task: None,
            watcher_started: false,
        };
        if app.config.theme == "omarchy" && app.omarchy.is_none() {
            app.err("Omarchy theme not found. Showing Catppuccin Mocha.");
        }
        app
    }

    pub fn bootstrap(&mut self) {
        self.probe();
        self.start_search();
        if self.watcher_started {
            return;
        }
        if let Some(path) = self.watch.clone() {
            let tx = self.events.clone();
            theme::spawn_watcher(path, move || {
                let _ = tx.send(Bus::OmarchyChanged);
            });
            self.watcher_started = true;
        }
    }

    pub fn on_bus(&mut self, event: Bus) {
        match event {
            Bus::Probe(ok) => {
                self.connected = Some(ok);
                if ok {
                    if self.status.starts_with("Ollama is not running") {
                        self.status.clear();
                        self.status_is_error = false;
                    }
                } else {
                    self.err(format!(
                        "Ollama is not running at {}. Start it in Settings.",
                        self.config.host
                    ));
                }
            }
            Bus::Service(state) => self.service = state,
            Bus::Tags(models) => {
                self.models = models;
                if self.model_index >= self.models.len() {
                    self.model_index = self.models.len().saturating_sub(1);
                }
                self.fill_empty_models();
                self.maybe_show();
            }
            Bus::Running(models) => self.running = models,
            Bus::Show { name, detail } => {
                if self.models.get(self.model_index).map(|m| m.name.as_str()) == Some(name.as_str())
                {
                    self.detail = Some(detail);
                }
            }
            Bus::ChatDelta {
                id,
                content,
                thinking,
            } => self.on_delta(id, content, thinking),
            Bus::ChatDone {
                id,
                tokens,
                tokens_per_sec,
            } => self.on_chat_done(id, tokens, tokens_per_sec),
            Bus::ChatErr { id, message } => self.on_chat_err(id, message),
            Bus::ChatIdle { id } => {
                if id == self.chat_job && self.streaming {
                    self.streaming = false;
                    self.streaming_chat = None;
                    self.persist_current();
                }
            }
            Bus::PullTick {
                id,
                status,
                completed,
                total,
            } => {
                if id == self.pull_job {
                    if let Some(pull) = self.pull.as_mut() {
                        if !status.is_empty() {
                            pull.status = status;
                        }
                        if total > 0 {
                            pull.completed = completed;
                            pull.total = total;
                        }
                    }
                }
            }
            Bus::PullDone { id } => {
                if id != self.pull_job {
                    return;
                }
                let label = self
                    .pull
                    .as_ref()
                    .map(|p| p.label.clone())
                    .unwrap_or_default();
                self.pull = None;
                self.info(format!("Pulled {label}"));
                self.probe();
            }
            Bus::PullErr { id, message } => {
                if id != self.pull_job {
                    return;
                }
                let message = ollama::short_pull_error(&message);
                if let Some(pull) = self.pull.as_mut() {
                    pull.status = message.clone();
                    pull.completed = 0;
                    pull.total = 0;
                }
                self.err(message);
            }
            Bus::SearchOk { id, models } => {
                if id != self.search_job {
                    return;
                }
                self.hf_loading = false;
                self.hf_results = models;
                self.hf_index = 0;
                if self.status.starts_with("Searching") {
                    self.status.clear();
                }
                if self.hf_pane == MarketPane::Search && !self.hf_results.is_empty() {
                    self.hf_pane = MarketPane::Results;
                }
            }
            Bus::SearchErr { id, message } => {
                if id != self.search_job {
                    return;
                }
                self.hf_loading = false;
                if self.screen == Screen::Marketplace {
                    self.err(message);
                }
            }
            Bus::FilesOk { id, repo, files } => {
                if id != self.files_job || repo != self.hf_repo {
                    return;
                }
                self.hf_loading = false;
                self.hf_files = files;
                self.hf_file_index = 0;
                if self.status.starts_with("Loading") {
                    self.status.clear();
                }
                if self.hf_files.is_empty() {
                    self.err("No GGUF files in that repository");
                }
            }
            Bus::FilesErr { id, message } => {
                if id != self.files_job {
                    return;
                }
                self.hf_loading = false;
                self.err(message);
            }
            Bus::Notice {
                error,
                message,
                refresh,
            } => {
                if error {
                    self.err(message);
                } else {
                    self.info(message);
                }
                if refresh {
                    self.probe();
                }
            }
            Bus::OmarchyChanged => self.reload_omarchy(),
        }
    }

    pub fn on_paste(&mut self, text: &str) {
        if self.help || self.confirm.is_some() {
            return;
        }
        let multiline = self.paste_is_multiline();
        let text = if multiline {
            text.to_string()
        } else {
            text.replace(['\n', '\r'], " ")
        };
        if let Some(field) = self.focused_field() {
            field.insert_str(&text);
        }
    }

    /// Returns true when the app should quit.
    pub fn on_key(&mut self, key: KeyEvent) -> bool {
        if key.kind == KeyEventKind::Release {
            return false;
        }
        if ctrl(&key, 'c') {
            self.shutdown();
            return true;
        }
        if self.help {
            match key.code {
                KeyCode::Esc | KeyCode::F(1) | KeyCode::Char('?') => self.help = false,
                KeyCode::Char('q') if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.shutdown();
                    return true;
                }
                _ => {}
            }
            return false;
        }
        if self.confirm.is_some() {
            match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => self.confirm_yes(),
                KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => {
                    self.confirm = None;
                }
                _ => {}
            }
            return false;
        }
        if key.code == KeyCode::Tab {
            self.switch_screen(self.screen.next());
            return false;
        }
        if key.code == KeyCode::BackTab {
            self.switch_screen(self.screen.prev());
            return false;
        }
        if key.code == KeyCode::F(1) {
            self.help = !self.help;
            return false;
        }
        if self.model_picker {
            self.key_model_picker(key);
            return false;
        }
        if self.renaming {
            self.key_rename(key);
            return false;
        }
        if self.editing_system {
            self.key_system(key);
            return false;
        }
        if matches!(key.code, KeyCode::Char('q')) && !self.typing() && key.modifiers.is_empty() {
            self.shutdown();
            return true;
        }
        if matches!(key.code, KeyCode::Char('?')) && !self.typing() && key.modifiers.is_empty() {
            self.help = true;
            return false;
        }
        match self.screen {
            Screen::Chat => self.key_chat(key),
            Screen::Models => self.key_models(key),
            Screen::Marketplace => self.key_market(key),
            Screen::Settings => self.key_settings(key),
        }
        if self.quit_requested {
            self.shutdown();
            return true;
        }
        false
    }

    pub fn scroll_lines(&mut self, delta: isize) {
        let view = self.transcript_view.max(1);
        let max = self.transcript_len.saturating_sub(view);
        if delta < 0 {
            self.stick_bottom = false;
            self.scroll = self.scroll.saturating_sub(delta.unsigned_abs());
        } else {
            let next = self.scroll.saturating_add(delta as usize);
            if next >= max {
                self.stick_bottom = true;
                self.scroll = max;
            } else {
                self.stick_bottom = false;
                self.scroll = next;
            }
        }
    }

    pub fn chat(&self) -> Option<&Conversation> {
        self.chats.get(self.chat_index)
    }

    pub fn model_choices(&self) -> Vec<String> {
        let query = self.model_filter.value.to_ascii_lowercase();
        self.models
            .iter()
            .map(|m| m.name.clone())
            .filter(|name| query.is_empty() || name.to_ascii_lowercase().contains(&query))
            .collect()
    }

    pub fn setting_value(&self, setting: Setting) -> String {
        match setting {
            Setting::Autostart => on_off(self.config.autostart_ollama),
            Setting::StopOnQuit => on_off(self.config.stop_ollama_on_quit),
            Setting::Theme => self.theme.name.clone(),
            Setting::Host => self.config.host.clone(),
            Setting::DefaultModel => {
                if self.config.default_model.is_empty() {
                    "—".into()
                } else {
                    self.config.default_model.clone()
                }
            }
            Setting::AssistantName => self.config.assistant_name.clone(),
            Setting::Temperature => format!("{:.2}", self.config.temperature),
            Setting::TopP => format!("{:.2}", self.config.top_p),
            Setting::NumCtx => self.config.num_ctx.to_string(),
            Setting::Seed => self
                .config
                .seed
                .map(|s| s.to_string())
                .unwrap_or_else(|| "random".into()),
            Setting::KeepAlive => self.config.keep_alive.clone(),
            Setting::TokenStats => {
                if self.config.show_token_stats {
                    "on".into()
                } else {
                    "off".into()
                }
            }
            Setting::Insecure => {
                if self.config.insecure_hf_pulls {
                    "on".into()
                } else {
                    "off".into()
                }
            }
            Setting::OpenConfig => config::config_dir().display().to_string(),
        }
    }

    pub fn preview_theme(&self, id: &str) -> Theme {
        if id == "omarchy" {
            self.omarchy
                .clone()
                .unwrap_or_else(|| theme::by_id("catppuccin-mocha").expect("mocha"))
        } else {
            theme::by_id(id).unwrap_or_else(|| self.theme.clone())
        }
    }

    fn shutdown(&mut self) {
        self.stop_generation();
        self.stash_draft();
        for index in 0..self.chats.len() {
            self.persist_index(index);
        }
    }

    fn switch_screen(&mut self, next: Screen) {
        if self.editing && !self.commit_edit() {
            return;
        }
        self.model_picker = false;
        self.renaming = false;
        self.editing_system = false;
        self.picking_theme = false;
        self.screen = next;
        if next == Screen::Models || next == Screen::Settings {
            self.probe();
        }
    }

    pub fn take_service_action(&mut self) -> Option<&'static str> {
        self.service_action.take()
    }

    pub fn finish_service(&mut self, result: Result<String, String>) {
        match result {
            Ok(message) => self.info(message),
            Err(message) => self.err(message),
        }
        self.probe();
    }

    fn typing(&self) -> bool {
        if self.editing || self.renaming || self.editing_system || self.model_picker {
            return true;
        }
        match self.screen {
            Screen::Chat => self.chat_pane == ChatPane::Composer,
            Screen::Models => self.models_pane == ModelsPane::Pull,
            Screen::Marketplace => self.hf_pane == MarketPane::Search,
            Screen::Settings => self.editing,
        }
    }

    fn paste_is_multiline(&self) -> bool {
        self.editing_system
            || (self.screen == Screen::Chat
                && self.chat_pane == ChatPane::Composer
                && !self.model_picker)
    }

    fn focused_field(&mut self) -> Option<&mut Field> {
        if self.model_picker {
            return Some(&mut self.model_filter);
        }
        if self.renaming {
            return Some(&mut self.rename_field);
        }
        if self.editing_system {
            return Some(&mut self.system_field);
        }
        if self.editing {
            return Some(&mut self.edit_field);
        }
        match self.screen {
            Screen::Chat if self.chat_pane == ChatPane::Composer => Some(&mut self.composer),
            Screen::Models if self.models_pane == ModelsPane::Pull => Some(&mut self.pull_field),
            Screen::Marketplace if self.hf_pane == MarketPane::Search => Some(&mut self.hf_query),
            _ => None,
        }
    }

    fn key_chat(&mut self, key: KeyEvent) {
        if ctrl(&key, 'n') {
            self.new_chat();
            return;
        }
        if ctrl(&key, 'm') {
            self.open_model_picker();
            return;
        }
        if ctrl(&key, 'p') {
            self.open_system();
            return;
        }
        if ctrl(&key, 'b') {
            self.toggle_chats();
            return;
        }
        match key.code {
            KeyCode::PageUp => self.page(-1),
            KeyCode::PageDown => self.page(1),
            _ => match self.chat_pane {
                ChatPane::Sidebar => self.key_sidebar(key),
                ChatPane::Transcript => self.key_transcript(key),
                ChatPane::Composer => self.key_composer(key),
            },
        }
    }

    fn key_sidebar(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => self.move_chat(1),
            KeyCode::Char('k') | KeyCode::Up => self.move_chat(-1),
            KeyCode::Enter | KeyCode::Char('i') | KeyCode::Char('l') | KeyCode::Right => {
                self.chat_pane = ChatPane::Composer;
            }
            KeyCode::Char('d') | KeyCode::Delete => self.ask_delete_chat(),
            KeyCode::Char('r') => self.open_rename(),
            _ => {}
        }
    }

    fn key_transcript(&mut self, key: KeyEvent) {
        let len = self.chat().map(|c| c.messages.len()).unwrap_or(0);
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                if len > 0 {
                    self.transcript_cursor = (self.transcript_cursor + 1).min(len - 1);
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.transcript_cursor = self.transcript_cursor.saturating_sub(1);
            }
            KeyCode::Char('h') | KeyCode::Left | KeyCode::Esc => {
                self.chat_pane = if self.show_chats {
                    ChatPane::Sidebar
                } else {
                    ChatPane::Composer
                };
            }
            KeyCode::Enter => self.toggle_thinking_or_compose(),
            KeyCode::Char('i') | KeyCode::Char('l') | KeyCode::Right => {
                self.chat_pane = ChatPane::Composer
            }
            _ => {}
        }
    }

    fn key_composer(&mut self, key: KeyEvent) {
        if self.model_palette_open() {
            let count = self.model_matches().len();
            match key.code {
                KeyCode::Up => {
                    self.slash_pick = self.slash_pick.saturating_sub(1);
                    return;
                }
                KeyCode::Down => {
                    if count > 0 {
                        self.slash_pick = (self.slash_pick + 1).min(count - 1);
                    }
                    return;
                }
                KeyCode::Tab | KeyCode::Enter if count > 0 => {
                    self.accept_model_palette();
                    return;
                }
                KeyCode::Esc => {
                    self.composer.clear();
                    self.slash_pick = 0;
                    return;
                }
                _ => {}
            }
        }
        if self.slash_palette_open() {
            let count = self.slash_matches().len();
            match key.code {
                KeyCode::Up => {
                    self.slash_pick = self.slash_pick.saturating_sub(1);
                    return;
                }
                KeyCode::Down => {
                    if count > 0 {
                        self.slash_pick = (self.slash_pick + 1).min(count - 1);
                    }
                    return;
                }
                KeyCode::Tab => {
                    self.complete_slash();
                    return;
                }
                KeyCode::Esc => {
                    self.composer.clear();
                    self.slash_pick = 0;
                    return;
                }
                KeyCode::Enter if !self.slash_query_exact() => {
                    self.complete_slash();
                    return;
                }
                _ => {}
            }
        }
        match key.code {
            KeyCode::Up if composer_at_edge(&self.composer, true) => {
                self.scroll_lines(-1);
                return;
            }
            KeyCode::Down if composer_at_edge(&self.composer, false) => {
                self.scroll_lines(1);
                return;
            }
            _ => {}
        }
        if key.code == KeyCode::Esc {
            if self.streaming {
                self.stop_generation();
            } else {
                self.chat_pane = if self.chat().map(|c| c.messages.is_empty()).unwrap_or(true) {
                    ChatPane::Sidebar
                } else {
                    ChatPane::Transcript
                };
            }
            return;
        }
        let before = self.composer.value.clone();
        match input(&mut self.composer, &key, true) {
            Input::Submit => self.send(),
            Input::Newline => self.composer.insert('\n'),
            Input::Cancel => self.chat_pane = ChatPane::Transcript,
            Input::Handled | Input::Ignored => {}
        }
        if self.composer.value != before {
            self.slash_pick = 0;
        }
    }

    pub fn slash_palette_open(&self) -> bool {
        self.slash_query().is_some()
    }

    pub fn model_palette_open(&self) -> bool {
        self.model_argument().is_some()
    }

    pub fn model_argument(&self) -> Option<&str> {
        if self.chat_pane != ChatPane::Composer
            || self.model_picker
            || self.renaming
            || self.editing_system
        {
            return None;
        }
        crate::slash::model_argument(&self.composer.value)
    }

    pub fn model_matches(&self) -> Vec<String> {
        let Some(query) = self.model_argument() else {
            return Vec::new();
        };
        if query.is_empty() {
            return self.models.iter().map(|model| model.name.clone()).collect();
        }
        let mut scored: Vec<(i32, String)> = self
            .models
            .iter()
            .filter_map(|model| {
                crate::slash::name_score(query, &model.name)
                    .map(|score| (score, model.name.clone()))
            })
            .collect();
        scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        scored.into_iter().map(|(_, name)| name).collect()
    }

    fn accept_model_palette(&mut self) {
        let matches = self.model_matches();
        let Some(name) = matches
            .get(self.slash_pick)
            .cloned()
            .or_else(|| matches.first().cloned())
        else {
            return;
        };
        self.composer.clear();
        self.slash_pick = 0;
        self.set_model(name);
    }

    fn toggle_chats(&mut self) {
        self.show_chats = !self.show_chats;
        if !self.show_chats && self.chat_pane == ChatPane::Sidebar {
            self.chat_pane = ChatPane::Composer;
        }
    }

    /// Text after `/` while the command name is still being typed.
    pub fn slash_query(&self) -> Option<&str> {
        if self.chat_pane != ChatPane::Composer
            || self.model_picker
            || self.renaming
            || self.editing_system
        {
            return None;
        }
        let value = self.composer.value.as_str();
        if value.contains(['\n', ' ', '\t']) {
            return None;
        }
        let rest = value.strip_prefix('/')?;
        if rest.chars().any(|ch| !ch.is_ascii_alphanumeric()) {
            return None;
        }
        Some(rest)
    }

    pub fn slash_matches(&self) -> Vec<&'static crate::slash::Command> {
        let Some(query) = self.slash_query() else {
            return Vec::new();
        };
        crate::slash::matching(query)
    }

    fn slash_query_exact(&self) -> bool {
        self.slash_query().is_some_and(crate::slash::is_exact)
    }

    fn complete_slash(&mut self) {
        let matches = self.slash_matches();
        let Some(command) = matches.get(self.slash_pick).or_else(|| matches.first()) else {
            return;
        };
        self.composer = crate::field::Field::new(crate::slash::completion(command));
        self.slash_pick = 0;
    }

    fn key_model_picker(&mut self, key: KeyEvent) {
        let count = self.model_choices().len();
        match key.code {
            KeyCode::Esc => self.model_picker = false,
            KeyCode::Up => self.model_pick = self.model_pick.saturating_sub(1),
            KeyCode::Down => {
                if count > 0 {
                    self.model_pick = (self.model_pick + 1).min(count - 1);
                }
            }
            KeyCode::Enter => self.apply_model_pick(),
            _ => {
                let before = self.model_filter.value.clone();
                if matches!(input(&mut self.model_filter, &key, false), Input::Submit) {
                    self.apply_model_pick();
                } else if self.model_filter.value != before {
                    self.model_pick = 0;
                }
            }
        }
    }

    fn key_rename(&mut self, key: KeyEvent) {
        match input(&mut self.rename_field, &key, false) {
            Input::Submit => {
                let title = self.rename_field.value.trim().to_string();
                if let Some(chat) = self.chats.get_mut(self.chat_index) {
                    if !title.is_empty() {
                        chat.title = title;
                        chat.updated_at = now_rfc3339();
                    }
                }
                self.renaming = false;
                self.persist_current();
            }
            Input::Cancel => self.renaming = false,
            _ => {}
        }
    }

    fn key_system(&mut self, key: KeyEvent) {
        if ctrl(&key, 's') {
            if let Some(chat) = self.chats.get_mut(self.chat_index) {
                chat.system = self.system_field.value.trim().to_string();
                chat.updated_at = now_rfc3339();
            }
            self.editing_system = false;
            self.persist_current();
            self.info("System prompt saved");
            return;
        }
        match input(&mut self.system_field, &key, true) {
            Input::Submit | Input::Newline => self.system_field.insert('\n'),
            Input::Cancel => self.editing_system = false,
            _ => {}
        }
    }

    fn key_models(&mut self, key: KeyEvent) {
        if self.models_pane == ModelsPane::Pull {
            if key.code == KeyCode::Esc {
                self.models_pane = ModelsPane::List;
                return;
            }
            match input(&mut self.pull_field, &key, false) {
                Input::Submit => self.pull_named(),
                Input::Cancel => self.models_pane = ModelsPane::List,
                _ => {}
            }
            return;
        }
        let len = self.models.len();
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                if len > 0 {
                    self.model_index = (self.model_index + 1).min(len - 1);
                    self.maybe_show();
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.model_index = self.model_index.saturating_sub(1);
                self.maybe_show();
            }
            KeyCode::Char('d') | KeyCode::Delete => self.ask_delete_model(),
            KeyCode::Char('u') => self.unload_selected(),
            KeyCode::Char('p') | KeyCode::Char('/') => self.models_pane = ModelsPane::Pull,
            KeyCode::Enter => self.use_selected_model(),
            _ => {}
        }
    }

    fn key_market(&mut self, key: KeyEvent) {
        if ctrl(&key, 's') {
            self.cycle_sort();
            return;
        }
        match self.hf_pane {
            MarketPane::Search => match input(&mut self.hf_query, &key, false) {
                Input::Submit => self.start_search(),
                Input::Cancel => {
                    if self.hf_query.value.is_empty() {
                        self.hf_pane = MarketPane::Results;
                    } else {
                        self.hf_query.clear();
                    }
                }
                _ => {}
            },
            MarketPane::Results => {
                let len = self.hf_results.len();
                match key.code {
                    KeyCode::Char('j') | KeyCode::Down => {
                        if len > 0 {
                            self.hf_index = (self.hf_index + 1).min(len - 1);
                        }
                    }
                    KeyCode::Char('k') | KeyCode::Up => {
                        self.hf_index = self.hf_index.saturating_sub(1)
                    }
                    KeyCode::Char('s') => self.cycle_sort(),
                    KeyCode::Char('/') => self.hf_pane = MarketPane::Search,
                    KeyCode::Enter => self.open_repo(),
                    KeyCode::Esc => self.hf_pane = MarketPane::Search,
                    _ => {}
                }
            }
            MarketPane::Files => {
                let len = self.hf_files.len();
                match key.code {
                    KeyCode::Char('j') | KeyCode::Down => {
                        if len > 0 {
                            self.hf_file_index = (self.hf_file_index + 1).min(len - 1);
                        }
                    }
                    KeyCode::Char('k') | KeyCode::Up => {
                        self.hf_file_index = self.hf_file_index.saturating_sub(1);
                    }
                    KeyCode::Enter => self.pull_selected_file(),
                    KeyCode::Esc => self.hf_pane = MarketPane::Results,
                    KeyCode::Char('s') => self.cycle_sort(),
                    _ => {}
                }
            }
        }
    }

    fn key_settings(&mut self, key: KeyEvent) {
        if self.picking_theme {
            self.key_theme_picker(key);
            return;
        }
        if self.editing {
            match input(&mut self.edit_field, &key, false) {
                Input::Submit => {
                    let _ = self.commit_edit();
                }
                Input::Cancel => self.editing = false,
                _ => {}
            }
            return;
        }
        let len = Setting::ALL.len();
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                self.settings_index = (self.settings_index + 1).min(len - 1);
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.settings_index = self.settings_index.saturating_sub(1);
            }
            KeyCode::Left => self.nudge(-1),
            KeyCode::Right => self.nudge(1),
            KeyCode::Enter | KeyCode::Char(' ') => self.activate_setting(),
            _ => {}
        }
    }

    fn key_theme_picker(&mut self, key: KeyEvent) {
        let len = theme::CHOICES.len();
        match key.code {
            KeyCode::Esc => self.picking_theme = false,
            KeyCode::Char('j') | KeyCode::Down => {
                if len > 0 {
                    self.theme_index = (self.theme_index + 1).min(len - 1);
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.theme_index = self.theme_index.saturating_sub(1)
            }
            KeyCode::Enter => {
                if let Some(choice) = theme::CHOICES.get(self.theme_index) {
                    self.apply_theme(choice.id);
                }
            }
            _ => {}
        }
    }

    fn activate_setting(&mut self) {
        match Setting::ALL[self.settings_index] {
            Setting::Autostart => {
                self.config.autostart_ollama = !self.config.autostart_ollama;
                self.save_config();
            }
            Setting::StopOnQuit => {
                self.config.stop_ollama_on_quit = !self.config.stop_ollama_on_quit;
                self.save_config();
            }
            Setting::Theme => {
                self.theme_index = theme::CHOICES
                    .iter()
                    .position(|c| c.id == self.config.theme)
                    .unwrap_or(0);
                self.picking_theme = true;
            }
            Setting::TokenStats => {
                self.config.show_token_stats = !self.config.show_token_stats;
                self.save_config();
            }
            Setting::Insecure => {
                self.config.insecure_hf_pulls = !self.config.insecure_hf_pulls;
                self.save_config();
            }
            Setting::OpenConfig => match config::open_config_dir() {
                Ok(()) => self.info("Opened the config folder"),
                Err(err) => self.err(err.to_string()),
            },
            Setting::Host
            | Setting::DefaultModel
            | Setting::AssistantName
            | Setting::Temperature
            | Setting::TopP
            | Setting::NumCtx
            | Setting::Seed
            | Setting::KeepAlive => self.begin_edit(),
        }
    }

    fn begin_edit(&mut self) {
        let text = match Setting::ALL[self.settings_index] {
            Setting::Host => self.config.host.clone(),
            Setting::DefaultModel => self.config.default_model.clone(),
            Setting::AssistantName => self.config.assistant_name.clone(),
            Setting::Temperature => format!("{:.2}", self.config.temperature),
            Setting::TopP => format!("{:.2}", self.config.top_p),
            Setting::NumCtx => self.config.num_ctx.to_string(),
            Setting::Seed => self.config.seed.map(|s| s.to_string()).unwrap_or_default(),
            Setting::KeepAlive => self.config.keep_alive.clone(),
            _ => return,
        };
        self.edit_field = Field::new(text);
        self.editing = true;
    }

    fn commit_edit(&mut self) -> bool {
        if !self.editing {
            return true;
        }
        let raw = self.edit_field.value.trim().to_string();
        match Setting::ALL[self.settings_index] {
            Setting::Host => {
                self.config.host = config::normalize_host(&raw);
                self.editing = false;
                self.save_config();
                self.probe();
            }
            Setting::DefaultModel => {
                self.config.default_model = raw;
                self.editing = false;
                self.save_config();
            }
            Setting::AssistantName => {
                self.config.assistant_name = if raw.is_empty() {
                    "assistant".into()
                } else {
                    raw
                };
                self.editing = false;
                self.save_config();
            }
            Setting::Temperature => match raw.parse::<f32>() {
                Ok(value) => {
                    self.config.temperature = value.clamp(0.0, 2.0);
                    self.editing = false;
                    self.save_config();
                }
                Err(_) => {
                    self.err("Temperature must be a number");
                    return false;
                }
            },
            Setting::TopP => match raw.parse::<f32>() {
                Ok(value) => {
                    self.config.top_p = value.clamp(0.0, 1.0);
                    self.editing = false;
                    self.save_config();
                }
                Err(_) => {
                    self.err("Top P must be a number");
                    return false;
                }
            },
            Setting::NumCtx => match raw.parse::<u32>() {
                Ok(value) if value >= 128 => {
                    self.config.num_ctx = value.min(262_144);
                    self.editing = false;
                    self.save_config();
                }
                _ => {
                    self.err("Context must be a number from 128 up");
                    return false;
                }
            },
            Setting::Seed => {
                if raw.is_empty() {
                    self.config.seed = None;
                } else {
                    match raw.parse::<i64>() {
                        Ok(value) => self.config.seed = Some(value),
                        Err(_) => {
                            self.err("Seed must be an integer or blank");
                            return false;
                        }
                    }
                }
                self.editing = false;
                self.save_config();
            }
            Setting::KeepAlive => {
                if raw.is_empty() {
                    self.err("Keep alive cannot be empty");
                    return false;
                }
                self.config.keep_alive = raw;
                self.editing = false;
                self.save_config();
            }
            _ => self.editing = false,
        }
        true
    }

    fn nudge(&mut self, dir: i32) {
        match Setting::ALL[self.settings_index] {
            Setting::Temperature => {
                self.config.temperature =
                    (self.config.temperature + dir as f32 * 0.1).clamp(0.0, 2.0);
            }
            Setting::TopP => {
                self.config.top_p = (self.config.top_p + dir as f32 * 0.05).clamp(0.0, 1.0);
            }
            Setting::NumCtx => {
                let next = self.config.num_ctx as i64 + dir as i64 * 1024;
                self.config.num_ctx = next.clamp(512, 262_144) as u32;
            }
            Setting::KeepAlive => {
                const PRESETS: [&str; 5] = ["0", "5m", "30m", "1h", "-1"];
                let pos = PRESETS
                    .iter()
                    .position(|p| *p == self.config.keep_alive)
                    .unwrap_or(1);
                let next = (pos as i32 + dir).clamp(0, PRESETS.len() as i32 - 1) as usize;
                self.config.keep_alive = PRESETS[next].into();
            }
            _ => return,
        }
        self.save_config();
    }

    fn apply_theme(&mut self, id: &str) {
        if id == "omarchy" && self.omarchy.is_none() {
            self.err("Omarchy is not installed");
            return;
        }
        self.config.theme = id.to_string();
        self.theme = theme::resolve(id, self.omarchy.as_ref());
        self.picking_theme = false;
        self.save_config();
        self.info(format!("Theme: {}", self.theme.name));
    }

    fn reload_omarchy(&mut self) {
        match theme::find_omarchy() {
            Some((name, theme, path)) => {
                self.omarchy_name = name;
                self.omarchy = Some(theme);
                self.watch = Some(path);
            }
            None => {
                self.omarchy = None;
                self.omarchy_name.clear();
            }
        }
        if self.config.theme == "omarchy" {
            self.theme = theme::resolve("omarchy", self.omarchy.as_ref());
        }
    }

    fn new_chat(&mut self) {
        if let Some(chat) = self.chat() {
            if chat.messages.is_empty() && chat.title == "New chat" {
                self.chat_pane = ChatPane::Composer;
                return;
            }
        }
        self.stash_draft();
        let model = self.preferred_model();
        self.chats.insert(0, Conversation::new(model));
        self.chat_index = 0;
        self.composer.clear();
        self.chat_pane = ChatPane::Composer;
        self.stick_bottom = true;
        self.scroll = 0;
        self.transcript_cursor = 0;
    }

    fn move_chat(&mut self, delta: isize) {
        if self.chats.is_empty() {
            return;
        }
        let next = if delta < 0 {
            self.chat_index.saturating_sub(1)
        } else {
            (self.chat_index + 1).min(self.chats.len() - 1)
        };
        if next != self.chat_index {
            self.select_chat(next);
        }
    }

    fn select_chat(&mut self, index: usize) {
        self.stash_draft();
        self.chat_index = index;
        let draft = self.chat().map(|c| c.draft.clone()).unwrap_or_default();
        self.composer = Field::new(draft);
        self.stick_bottom = true;
        self.scroll = 0;
        self.transcript_cursor = self
            .chat()
            .map(|c| c.messages.len().saturating_sub(1))
            .unwrap_or(0);
    }

    fn stash_draft(&mut self) {
        if let Some(chat) = self.chats.get_mut(self.chat_index) {
            chat.draft = self.composer.value.clone();
        }
    }

    fn send(&mut self) {
        let text = self.composer.value.trim().to_string();
        if text.is_empty() {
            return;
        }
        if text.starts_with('/') {
            self.composer.clear();
            if let Some(chat) = self.chats.get_mut(self.chat_index) {
                chat.draft.clear();
            }
            self.slash(&text);
            return;
        }
        if self.streaming {
            self.info("Already generating. Esc stops it.");
            return;
        }
        if self.connected == Some(false) {
            self.err(format!(
                "Ollama is not running at {host}. Type /start.",
                host = self.config.host
            ));
            return;
        }
        let model = self.chat().map(|c| c.model.clone()).unwrap_or_default();
        let model = if model.is_empty() {
            self.preferred_model()
        } else {
            model
        };
        if model.is_empty() {
            self.err("Pick a model with Ctrl+M, or pull one from Models.");
            return;
        }
        let id = self.chat().map(|c| c.id.clone()).unwrap_or_default();
        if let Some(chat) = self.chats.get_mut(self.chat_index) {
            chat.model = model;
            chat.draft.clear();
            chat.messages.push(Message::user(text));
            chat.messages.push(Message::assistant());
            chat.touch_title();
            chat.updated_at = now_rfc3339();
            self.transcript_cursor = chat.messages.len().saturating_sub(1);
        }
        self.composer.clear();
        self.stick_bottom = true;
        self.streaming = true;
        self.streaming_chat = Some(id);
        self.bump_chat_to_top();
        self.persist_current();
        self.spawn_chat();
    }

    fn bump_chat_to_top(&mut self) {
        if self.chat_index == 0 || self.chat_index >= self.chats.len() {
            return;
        }
        let chat = self.chats.remove(self.chat_index);
        self.chats.insert(0, chat);
        self.chat_index = 0;
    }

    fn stop_generation(&mut self) {
        self.chat_job = self.chat_job.wrapping_add(1);
        if let Some(task) = self.chat_task.take() {
            task.abort();
        }
        if self.streaming {
            self.streaming = false;
            self.streaming_chat = None;
            self.persist_current();
            self.info("Stopped");
        }
    }

    fn spawn_chat(&mut self) {
        self.chat_job = self.chat_job.wrapping_add(1);
        let id = self.chat_job;
        if let Some(task) = self.chat_task.take() {
            task.abort();
        }
        let Some(chat) = self.chat().cloned() else {
            return;
        };
        let host = self.config.host.clone();
        let params = ChatParams {
            temperature: self.config.temperature,
            top_p: self.config.top_p,
            num_ctx: self.config.num_ctx,
            seed: self.config.seed,
            keep_alive: self.config.keep_alive.clone(),
        };
        let turns: Vec<ChatTurn> = chat
            .messages
            .iter()
            .rev()
            .skip(1)
            .rev()
            .filter(|m| !m.content.is_empty())
            .map(|m| ChatTurn {
                role: m.role.clone(),
                content: m.content.clone(),
            })
            .collect();
        let model = chat.model.clone();
        let system = chat.system.clone();
        let tx = self.events.clone();
        self.chat_task = Some(tokio::spawn(async move {
            let (local_tx, mut local_rx) = tokio::sync::mpsc::unbounded_channel();
            let mut request = Box::pin(ollama::stream_chat(
                &host, &model, &system, &turns, &params, &local_tx,
            ));
            let mut finished: Option<Result<(), String>> = None;
            loop {
                tokio::select! {
                    biased;
                    // Observe completion before the closed channel, or an error
                    // would be reported as a clean finish.
                    result = &mut request, if finished.is_none() => {
                        finished = Some(result.map_err(|err| format!("{err:#}")));
                    }
                    event = local_rx.recv() => {
                        match event {
                            Some(ChatEvent::Delta { content, thinking }) => {
                                if tx.send(Bus::ChatDelta { id, content, thinking }).is_err() {
                                    break;
                                }
                            }
                            Some(ChatEvent::Done(done)) => {
                                let _ = tx.send(Bus::ChatDone {
                                    id,
                                    tokens: done.tokens,
                                    tokens_per_sec: done.tokens_per_sec,
                                });
                            }
                            None => {
                                match finished {
                                    Some(Err(message)) => {
                                        let _ = tx.send(Bus::ChatErr { id, message });
                                    }
                                    _ => {
                                        let _ = tx.send(Bus::ChatIdle { id });
                                    }
                                }
                                break;
                            }
                        }
                    }
                }
            }
        }));
    }

    fn on_delta(&mut self, id: u64, content: String, thinking: String) {
        if id != self.chat_job {
            return;
        }
        let Some(chat_id) = self.streaming_chat.clone() else {
            return;
        };
        if let Some(chat) = self.chats.iter_mut().find(|c| c.id == chat_id) {
            if let Some(message) = chat.messages.last_mut() {
                message.content.push_str(&content);
                message.thinking.push_str(&thinking);
            }
        }
    }

    fn on_chat_done(&mut self, id: u64, tokens: u64, tokens_per_sec: f64) {
        if id != self.chat_job {
            return;
        }
        if let Some(chat_id) = self.streaming_chat.clone() {
            if let Some(chat) = self.chats.iter_mut().find(|c| c.id == chat_id) {
                if let Some(message) = chat.messages.last_mut() {
                    if tokens > 0 {
                        message.stats = Some(store::Stats {
                            tokens,
                            tokens_per_sec,
                        });
                    }
                }
                chat.updated_at = now_rfc3339();
            }
        }
        self.streaming = false;
        self.streaming_chat = None;
        self.persist_current();
        self.refresh_running();
    }

    fn on_chat_err(&mut self, id: u64, message: String) {
        if id != self.chat_job {
            return;
        }
        self.streaming = false;
        self.streaming_chat = None;
        self.persist_current();
        self.err(message);
    }

    fn toggle_thinking_or_compose(&mut self) {
        let index = self.transcript_cursor;
        let has_thinking = self.chat().and_then(|c| c.messages.get(index)).map(|m| {
            let (tagged, _) = crate::util::split_think(&m.content);
            !m.thinking.trim().is_empty() || !tagged.trim().is_empty()
        });
        if has_thinking == Some(true) {
            if let Some(chat) = self.chats.get_mut(self.chat_index) {
                if let Some(message) = chat.messages.get_mut(index) {
                    message.thinking_open = !message.thinking_open;
                }
            }
        } else {
            self.chat_pane = ChatPane::Composer;
        }
    }

    fn open_model_picker(&mut self) {
        self.model_filter.clear();
        self.model_pick = 0;
        self.model_picker = true;
        if self.models.is_empty() {
            self.probe();
        }
    }

    fn apply_model_pick(&mut self) {
        let choices = self.model_choices();
        let Some(name) = choices.get(self.model_pick).cloned() else {
            self.err("No installed models. Pull one from Models or Marketplace.");
            return;
        };
        if let Some(chat) = self.chats.get_mut(self.chat_index) {
            chat.model = name.clone();
        }
        if self.config.default_model.is_empty() {
            self.config.default_model = name.clone();
            self.save_config();
        }
        self.model_picker = false;
        self.info(format!("Model: {name}"));
    }

    fn open_system(&mut self) {
        let current = self.chat().map(|c| c.system.clone()).unwrap_or_default();
        self.system_field = Field::new(current);
        self.editing_system = true;
    }

    fn open_rename(&mut self) {
        let title = self.chat().map(|c| c.title.clone()).unwrap_or_default();
        self.rename_field = Field::new(title);
        self.renaming = true;
    }

    fn ask_delete_chat(&mut self) {
        let Some(chat) = self.chat() else { return };
        self.confirm = Some(Confirm::DeleteChat {
            id: chat.id.clone(),
            title: chat.title.clone(),
        });
    }

    fn ask_delete_model(&mut self) {
        let Some(model) = self.models.get(self.model_index) else {
            self.info("No models to delete");
            return;
        };
        self.confirm = Some(Confirm::DeleteModel(model.name.clone()));
    }

    fn confirm_yes(&mut self) {
        match self.confirm.take() {
            Some(Confirm::DeleteChat { id, .. }) => self.delete_chat(&id),
            Some(Confirm::DeleteModel(name)) => self.spawn_delete_model(name),
            None => {}
        }
    }

    fn delete_chat(&mut self, id: &str) {
        if self.streaming_chat.as_deref() == Some(id) {
            self.stop_generation();
        }
        let persisted = self.chats.iter().any(|c| c.id == id && c.persisted);
        self.chats.retain(|c| c.id != id);
        if persisted {
            let _ = store::delete(id);
        }
        if self.chats.is_empty() {
            self.chats
                .push(Conversation::new(self.config.default_model.clone()));
        }
        if self.chat_index >= self.chats.len() {
            self.chat_index = self.chats.len() - 1;
        }
        let draft = self.chat().map(|c| c.draft.clone()).unwrap_or_default();
        self.composer = Field::new(draft);
        self.stick_bottom = true;
    }

    fn spawn_delete_model(&mut self, name: String) {
        let host = self.config.host.clone();
        let tx = self.events.clone();
        let label = name.clone();
        tokio::spawn(async move {
            match ollama::delete_model(&host, &name).await {
                Ok(()) => {
                    let _ = tx.send(Bus::Notice {
                        error: false,
                        message: format!("Deleted {label}"),
                        refresh: true,
                    });
                }
                Err(err) => {
                    let _ = tx.send(Bus::Notice {
                        error: true,
                        message: format!("{err:#}"),
                        refresh: false,
                    });
                }
            }
        });
    }

    fn unload_selected(&mut self) {
        let Some(model) = self.models.get(self.model_index) else {
            return;
        };
        let name = model.name.clone();
        if !self.running.iter().any(|m| m.name == name) {
            self.info(format!("{name} is not loaded"));
            return;
        }
        let host = self.config.host.clone();
        let tx = self.events.clone();
        tokio::spawn(async move {
            match ollama::unload(&host, &name).await {
                Ok(()) => {
                    let _ = tx.send(Bus::Notice {
                        error: false,
                        message: format!("Unloaded {name}"),
                        refresh: true,
                    });
                }
                Err(err) => {
                    let _ = tx.send(Bus::Notice {
                        error: true,
                        message: format!("{err:#}"),
                        refresh: false,
                    });
                }
            }
        });
    }

    fn use_selected_model(&mut self) {
        let Some(name) = self.models.get(self.model_index).map(|m| m.name.clone()) else {
            return;
        };
        if let Some(chat) = self.chats.get_mut(self.chat_index) {
            chat.model = name.clone();
        }
        self.screen = Screen::Chat;
        self.chat_pane = ChatPane::Composer;
        self.info(format!("Chatting with {name}"));
    }

    fn pull_named(&mut self) {
        let name = self.pull_field.value.trim().to_string();
        if name.is_empty() {
            return;
        }
        let insecure = name.starts_with("hf.co/") && self.config.insecure_hf_pulls;
        self.start_pull(name, insecure);
        self.models_pane = ModelsPane::List;
    }

    fn pull_selected_file(&mut self) {
        let Some(file) = self.hf_files.get(self.hf_file_index) else {
            return;
        };
        if self.connected == Some(false) {
            self.err("Ollama is not running, so the download cannot start");
            return;
        }
        let reference = hf::ollama_ref(&self.hf_repo, &file.path);
        let insecure = self.config.insecure_hf_pulls;
        self.start_pull(reference, insecure);
    }

    fn start_pull(&mut self, model: String, insecure: bool) {
        self.pull_job = self.pull_job.wrapping_add(1);
        let id = self.pull_job;
        if let Some(task) = self.pull_task.take() {
            task.abort();
        }
        self.pull = Some(PullView {
            label: model.clone(),
            status: "starting".into(),
            completed: 0,
            total: 0,
        });
        let host = self.config.host.clone();
        let tx = self.events.clone();
        self.pull_task = Some(tokio::spawn(async move {
            drive_pull(host, model, insecure, id, tx).await;
        }));
    }

    fn start_search(&mut self) {
        self.search_job = self.search_job.wrapping_add(1);
        let id = self.search_job;
        if let Some(task) = self.search_task.take() {
            task.abort();
        }
        let query = self.hf_query.value.clone();
        let sort = self.hf_sort;
        let tx = self.events.clone();
        self.hf_loading = true;
        if self.screen == Screen::Marketplace {
            self.info("Searching Hugging Face…");
        }
        self.search_task = Some(tokio::spawn(async move {
            match hf::search(&query, sort).await {
                Ok(models) => {
                    let _ = tx.send(Bus::SearchOk { id, models });
                }
                Err(err) => {
                    let _ = tx.send(Bus::SearchErr {
                        id,
                        message: format!("{err:#}"),
                    });
                }
            }
        }));
    }

    fn cycle_sort(&mut self) {
        self.hf_sort = self.hf_sort.cycle();
        self.info(format!("Sort: {}", self.hf_sort.label()));
        self.start_search();
    }

    fn open_repo(&mut self) {
        let Some(model) = self.hf_results.get(self.hf_index) else {
            return;
        };
        let repo = model.id.clone();
        self.hf_repo = repo.clone();
        self.hf_files.clear();
        self.hf_file_index = 0;
        self.hf_pane = MarketPane::Files;
        self.files_job = self.files_job.wrapping_add(1);
        let id = self.files_job;
        if let Some(task) = self.files_task.take() {
            task.abort();
        }
        let tx = self.events.clone();
        self.hf_loading = true;
        self.info(format!("Loading {repo}…"));
        self.files_task = Some(tokio::spawn(async move {
            match hf::gguf_files(&repo).await {
                Ok(files) => {
                    let _ = tx.send(Bus::FilesOk { id, repo, files });
                }
                Err(err) => {
                    let _ = tx.send(Bus::FilesErr {
                        id,
                        message: format!("{err:#}"),
                    });
                }
            }
        }));
    }

    fn page(&mut self, delta: isize) {
        let view = self.transcript_view.max(1) as isize;
        self.scroll_lines(delta * view);
    }

    fn slash(&mut self, line: &str) {
        let mut parts = line.splitn(2, char::is_whitespace);
        let cmd = parts
            .next()
            .unwrap_or("")
            .trim_start_matches('/')
            .to_ascii_lowercase();
        let arg = parts.next().unwrap_or("").trim();
        match cmd.as_str() {
            "start" => self.service_action = Some("start"),
            "stop" => self.service_action = Some("stop"),
            "restart" => self.service_action = Some("restart"),
            "model" => self.slash_model(arg),
            "chats" => self.toggle_chats(),
            "new" => self.new_chat(),
            "clear" => self.clear_chat(),
            "cancel" => self.stop_generation(),
            "system" => self.slash_system(arg),
            "theme" => self.slash_theme(arg),
            "pull" => {
                if arg.is_empty() {
                    self.err("Usage: /pull name");
                } else {
                    self.start_pull(arg.to_string(), false);
                }
            }
            "help" | "commands" | "?" | "" => self.help = true,
            "quit" | "exit" | "q" => self.quit_requested = true,
            other => self.err(format!("Unknown command /{other}. /help lists commands.")),
        }
    }

    fn slash_model(&mut self, arg: &str) {
        if arg.is_empty() {
            self.open_model_picker();
            return;
        }
        let query = arg.to_ascii_lowercase();
        let matches: Vec<String> = self
            .models
            .iter()
            .filter(|model| model.name.to_ascii_lowercase().contains(&query))
            .map(|model| model.name.clone())
            .collect();
        match matches.as_slice() {
            [only] => self.set_model(only.clone()),
            [] => {
                self.model_filter = crate::field::Field::new(arg);
                self.model_pick = 0;
                self.model_picker = true;
                self.err(format!("No installed model matches {arg}"));
            }
            _ => {
                self.model_filter = crate::field::Field::new(arg);
                self.model_pick = 0;
                self.model_picker = true;
            }
        }
    }

    fn set_model(&mut self, name: String) {
        if let Some(chat) = self.chats.get_mut(self.chat_index) {
            chat.model = name.clone();
        }
        if self.config.default_model.is_empty() {
            self.config.default_model = name.clone();
            self.save_config();
        }
        self.info(format!("Model: {name}"));
    }

    fn slash_system(&mut self, arg: &str) {
        if arg.is_empty() {
            self.open_system();
            return;
        }
        if let Some(chat) = self.chats.get_mut(self.chat_index) {
            chat.system = arg.to_string();
            chat.updated_at = now_rfc3339();
        }
        self.persist_current();
        self.info("System prompt saved");
    }

    fn slash_theme(&mut self, arg: &str) {
        if arg.is_empty() {
            self.info(format!(
                "Theme: {}. /theme <name> to change it.",
                self.theme.name
            ));
            return;
        }
        let needle = arg.to_ascii_lowercase();
        let choice = theme::CHOICES.iter().find(|choice| {
            choice.id == needle
                || choice.label.eq_ignore_ascii_case(arg)
                || choice.label.to_ascii_lowercase().contains(&needle)
        });
        match choice {
            Some(choice) => self.apply_theme(choice.id),
            None => self.err(format!("Unknown theme {arg}")),
        }
    }

    fn clear_chat(&mut self) {
        if let Some(chat) = self.chats.get_mut(self.chat_index) {
            chat.messages.clear();
            chat.updated_at = now_rfc3339();
        }
        self.stick_bottom = true;
        self.scroll = 0;
        self.transcript_cursor = 0;
        self.persist_current();
        self.info("Cleared this chat");
    }

    fn preferred_model(&self) -> String {
        if !self.config.default_model.is_empty() {
            return self.config.default_model.clone();
        }
        if let Some(chat) = self.chat() {
            if !chat.model.is_empty() {
                return chat.model.clone();
            }
        }
        self.models
            .first()
            .map(|m| m.name.clone())
            .unwrap_or_default()
    }

    fn fill_empty_models(&mut self) {
        let fallback = if !self.config.default_model.is_empty() {
            self.config.default_model.clone()
        } else {
            self.models
                .first()
                .map(|m| m.name.clone())
                .unwrap_or_default()
        };
        if fallback.is_empty() {
            return;
        }
        for chat in &mut self.chats {
            if chat.model.is_empty() {
                chat.model = fallback.clone();
            }
        }
    }

    fn maybe_show(&mut self) {
        let Some(model) = self.models.get(self.model_index) else {
            return;
        };
        if self.shown_for == model.name {
            return;
        }
        let name = model.name.clone();
        self.shown_for = name.clone();
        let host = self.config.host.clone();
        let tx = self.events.clone();
        tokio::spawn(async move {
            if let Ok(detail) = ollama::show(&host, &name).await {
                let _ = tx.send(Bus::Show { name, detail });
            }
        });
    }

    fn probe(&mut self) {
        let host = self.config.host.clone();
        let tx = self.events.clone();
        tokio::spawn(async move {
            match ollama::tags(&host).await {
                Ok(models) => {
                    let _ = tx.send(Bus::Probe(true));
                    let _ = tx.send(Bus::Tags(models));
                }
                Err(_) => {
                    let _ = tx.send(Bus::Probe(false));
                }
            }
        });
        self.refresh_running();
        self.refresh_service();
    }

    fn refresh_service(&mut self) {
        let tx = self.events.clone();
        tokio::spawn(async move {
            let state = tokio::task::spawn_blocking(ollama::service_state)
                .await
                .unwrap_or_else(|_| "unknown".into());
            let _ = tx.send(Bus::Service(state));
        });
    }

    fn refresh_running(&mut self) {
        let host = self.config.host.clone();
        let tx = self.events.clone();
        tokio::spawn(async move {
            if let Ok(models) = ollama::running(&host).await {
                let _ = tx.send(Bus::Running(models));
            }
        });
    }

    fn persist_current(&mut self) {
        let index = self.chat_index;
        self.persist_index(index);
    }

    fn persist_index(&mut self, index: usize) {
        let Some(chat) = self.chats.get_mut(index) else {
            return;
        };
        if chat.messages.is_empty() && !chat.persisted {
            return;
        }
        if store::save(chat).is_ok() {
            chat.persisted = true;
        }
    }

    fn save_config(&mut self) {
        if let Err(err) = self.config.save(&self.config_path) {
            self.err(format!("Could not save config: {err}"));
        }
    }

    fn info(&mut self, message: impl Into<String>) {
        self.status = message.into();
        self.status_is_error = false;
    }

    fn err(&mut self, message: impl Into<String>) {
        self.status = message.into();
        self.status_is_error = true;
    }
}

enum Input {
    Handled,
    Ignored,
    Submit,
    Cancel,
    Newline,
}

fn input(field: &mut Field, key: &KeyEvent, multiline: bool) -> Input {
    match key.code {
        KeyCode::Char(c) if key.modifiers.contains(KeyModifiers::CONTROL) => match c {
            'a' => {
                field.home_line();
                Input::Handled
            }
            'e' => {
                field.end_line();
                Input::Handled
            }
            'u' => {
                field.clear_to_start();
                Input::Handled
            }
            'w' => {
                field.back_word();
                Input::Handled
            }
            'j' if multiline => Input::Newline,
            _ => Input::Ignored,
        },
        KeyCode::Char(_) if key.modifiers.contains(KeyModifiers::ALT) => Input::Ignored,
        KeyCode::Char(c) => {
            field.insert(c);
            Input::Handled
        }
        KeyCode::Backspace => {
            field.backspace();
            Input::Handled
        }
        KeyCode::Delete => {
            field.delete();
            Input::Handled
        }
        KeyCode::Left => {
            field.left();
            Input::Handled
        }
        KeyCode::Right => {
            field.right();
            Input::Handled
        }
        KeyCode::Home => {
            field.home_line();
            Input::Handled
        }
        KeyCode::End => {
            field.end_line();
            Input::Handled
        }
        KeyCode::Up => {
            if multiline {
                field.up();
            }
            Input::Handled
        }
        KeyCode::Down => {
            if multiline {
                field.down();
            }
            Input::Handled
        }
        KeyCode::Enter
            if key.modifiers.contains(KeyModifiers::ALT)
                || key.modifiers.contains(KeyModifiers::SHIFT) =>
        {
            Input::Newline
        }
        KeyCode::Enter => Input::Submit,
        KeyCode::Esc => Input::Cancel,
        _ => Input::Ignored,
    }
}

async fn drive_pull(
    host: String,
    model: String,
    mut insecure: bool,
    id: u64,
    tx: tokio::sync::mpsc::UnboundedSender<Bus>,
) {
    let huggingface = model.starts_with("hf.co/") || model.starts_with("huggingface.co/");
    // The manifest is on hf.co. The file is then redirected to a CDN host,
    // which Ollama rejects unless this flag is set. Without it the pull
    // stops on "pulling manifest".
    if huggingface {
        insecure = true;
    }
    let (local_tx, mut local_rx) = tokio::sync::mpsc::unbounded_channel();
    let host_for_pull = host.clone();
    let model_for_pull = model.clone();
    let attempt = tokio::spawn(async move {
        ollama::stream_pull(&host_for_pull, &model_for_pull, insecure, &local_tx)
            .await
            .map_err(|err| format!("{err:#}"))
    });
    let mut saw_success = false;
    while let Some(tick) = local_rx.recv().await {
        if tick.status == "success" {
            saw_success = true;
        }
        if tx
            .send(Bus::PullTick {
                id,
                status: tick.status,
                completed: tick.completed,
                total: tick.total,
            })
            .is_err()
        {
            attempt.abort();
            return;
        }
    }
    let outcome = match attempt.await {
        Ok(result) => result,
        Err(err) => Err(err.to_string()),
    };
    match outcome {
        Ok(()) if saw_success => {
            let _ = tx.send(Bus::PullDone { id });
        }
        Ok(()) => {
            let _ = tx.send(Bus::PullErr {
                id,
                message: "Download stopped before it finished.".into(),
            });
        }
        Err(message) => {
            let _ = tx.send(Bus::PullErr { id, message });
        }
    }
}

fn on_off(value: bool) -> String {
    if value {
        "on".into()
    } else {
        "off".into()
    }
}

fn composer_at_edge(field: &crate::field::Field, first: bool) -> bool {
    if first {
        !field.value[..field.cursor].contains('\n')
    } else {
        !field.value[field.cursor..].contains('\n')
    }
}

fn ctrl(key: &KeyEvent, c: char) -> bool {
    matches!(key.code, KeyCode::Char(ch) if ch == c)
        && key.modifiers.contains(KeyModifiers::CONTROL)
}
