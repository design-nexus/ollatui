mod app;
mod config;
mod field;
mod hf;
mod ollama;
mod store;
mod theme;
mod ui;
mod util;

use std::io::stdout;

use anyhow::Context;
use crossterm::event::{
    DisableMouseCapture, EnableMouseCapture, Event, EventStream, MouseEventKind,
};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use futures_util::StreamExt;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tokio::sync::mpsc::unbounded_channel;

use crate::app::App;

struct Suspended;

impl Suspended {
    fn enter() -> Self {
        let _ = disable_raw_mode();
        let _ = execute!(stdout(), LeaveAlternateScreen, DisableMouseCapture);
        Self
    }
}

impl Drop for Suspended {
    fn drop(&mut self) {
        let _ = enable_raw_mode();
        let _ = execute!(stdout(), EnterAlternateScreen, EnableMouseCapture);
    }
}

struct TtyGuard;

impl Drop for TtyGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(stdout(), LeaveAlternateScreen, DisableMouseCapture);
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let found = theme::find_omarchy();
    let config_path = config::config_path();
    let cfg = config::Config::load(&config_path, found.is_some());
    let (omarchy_name, omarchy, watch) = match found {
        Some((name, theme, path)) => (name, Some(theme), Some(path)),
        None => (String::new(), None, None),
    };
    let chats = store::load_all();
    let (tx, mut rx) = unbounded_channel();
    let mut app = App::new(cfg, config_path, chats, omarchy, omarchy_name, watch, tx);
    app.bootstrap();
    if app.config.autostart_ollama {
        let state = ollama::service_state();
        if !matches!(state.as_str(), "active" | "running" | "activating") {
            println!("Starting Ollama…");
            if let Err(err) = ollama::control("start") {
                eprintln!("{err}");
            }
        }
    }

    enable_raw_mode().context("failed to put the terminal in raw mode")?;
    execute!(stdout(), EnterAlternateScreen, EnableMouseCapture)
        .context("failed to enter the alternate screen")?;
    let _guard = TtyGuard;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    let mut reader = EventStream::new();

    let status = loop {
        if let Err(err) = terminal.draw(|frame| ui::draw(frame, &mut app)) {
            break Err(err).context("drawing the screen");
        }
        tokio::select! {
            biased;
            event = reader.next() => {
                match event {
                    Some(Ok(Event::Key(key))) => {
                        if app.on_key(key) {
                            break Ok(());
                        }
                        if let Some(action) = app.take_service_action() {
                            let suspended = Suspended::enter();
                            println!();
                            let result = ollama::control(action);
                            drop(suspended);
                            if let Err(err) = terminal.clear() {
                                break Err(err).context("clearing the screen");
                            }
                            app.finish_service(result);
                        }
                    }
                    Some(Ok(Event::Paste(text))) => app.on_paste(&text),
                    Some(Ok(Event::Mouse(mouse))) => {
                        if app.screen == crate::app::Screen::Chat {
                            match mouse.kind {
                                MouseEventKind::ScrollUp => app.scroll_lines(-3),
                                MouseEventKind::ScrollDown => app.scroll_lines(3),
                                _ => {}
                            }
                        }
                    }
                    Some(Ok(_)) => {}
                    Some(Err(err)) => break Err(err).context("reading terminal input"),
                    None => break Ok(()),
                }
            }
            event = rx.recv() => {
                match event {
                    Some(bus) => app.on_bus(bus),
                    None => break Ok(()),
                }
            }
        }
    };
    drop(terminal);
    let _ = disable_raw_mode();
    let _ = execute!(stdout(), LeaveAlternateScreen, DisableMouseCapture);
    if app.config.stop_ollama_on_quit {
        println!();
        if let Err(err) = ollama::control("stop") {
            eprintln!("{err}");
        }
    }
    status
}
