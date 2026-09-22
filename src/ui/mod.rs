mod chat;
mod marketplace;
mod models;
mod settings;

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Gauge, Paragraph};
use ratatui::Frame;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::app::{App, Screen};
use crate::field::Field;
use crate::theme::Theme;
use crate::util::{human_bytes, truncate};

pub fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    if area.width < 2 || area.height < 2 {
        return;
    }
    let theme = app.theme.clone();
    frame.render_widget(Block::default().style(theme.base()), area);

    let footer_h = if app.pull.is_some() { 2 } else { 1 };
    let [status, tabs, body, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(footer_h),
    ])
    .areas(area);

    draw_status(frame, app, &theme, status);
    draw_tabs(frame, app, &theme, tabs);
    match app.screen {
        Screen::Chat => chat::draw(frame, app, body),
        Screen::Models => models::draw(frame, app, body),
        Screen::Marketplace => marketplace::draw(frame, app, body),
        Screen::Settings => settings::draw(frame, app, body),
    }
    draw_footer(frame, app, &theme, footer);

    if app.model_picker {
        chat::draw_model_picker(frame, app, area);
    }
    if app.renaming {
        chat::draw_rename(frame, app, area);
    }
    if app.editing_system {
        chat::draw_system(frame, app, area);
    }
    if let Some(confirm) = app.confirm.as_ref() {
        draw_confirm(frame, &theme, area, confirm);
    }
    if app.help {
        draw_help(frame, &theme, area);
    }
}

fn draw_status(frame: &mut Frame, app: &App, theme: &Theme, area: Rect) {
    let model = short_model(
        app.chat()
            .map(|c| c.model.as_str())
            .filter(|m| !m.is_empty())
            .unwrap_or("no model"),
    );
    let status_style = if app.status_is_error {
        Style::default().fg(theme.red)
    } else {
        Style::default().fg(theme.fg_dim)
    };
    let mut spans = vec![
        Span::styled(
            " ollatui ",
            theme.accent_style().add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!(" {model} "), theme.status()),
    ];
    let used: usize = spans
        .iter()
        .map(|s| UnicodeWidthStr::width(s.content.as_ref()))
        .sum();
    let room = (area.width as usize).saturating_sub(used + 1);
    if !app.status.is_empty() && room > 4 {
        spans.push(Span::styled(
            format!(" {} ", truncate(&app.status, room)),
            status_style,
        ));
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(theme.status()),
        area,
    );
}

fn short_model(name: &str) -> String {
    let tail = name.rsplit('/').next().unwrap_or(name);
    truncate(tail, 18)
}

pub fn connection(app: &App, theme: &Theme) -> (String, ratatui::style::Color) {
    match app.connected {
        Some(true) => ("connected".into(), theme.green),
        Some(false) => ("offline".into(), theme.red),
        None => ("connecting".into(), theme.yellow),
    }
}

fn draw_tabs(frame: &mut Frame, app: &App, theme: &Theme, area: Rect) {
    let mut spans = Vec::new();
    for (index, screen) in Screen::ALL.iter().enumerate() {
        if index > 0 {
            spans.push(Span::styled("  ", theme.base()));
        }
        let style = if *screen == app.screen {
            Style::default()
                .fg(theme.accent)
                .bg(theme.bg)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
        } else {
            Style::default().fg(theme.muted).bg(theme.bg)
        };
        spans.push(Span::styled(format!(" {} ", screen.label()), style));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)).style(theme.base()), area);
}

fn draw_footer(frame: &mut Frame, app: &App, theme: &Theme, area: Rect) {
    let hint = match app.screen {
        Screen::Chat => "wheel or pgup scrolls  up/down at the prompt  /help",
        Screen::Models => "j/k move  enter use  d delete  u unload  p pull",
        Screen::Marketplace => {
            "enter search, open, or pull  ^S sort  esc back   gated models need HF_TOKEN"
        }
        Screen::Settings => "j/k move  enter edit  ← → nudge  esc cancel",
    };
    if let Some(pull) = app.pull.as_ref() {
        let [gauge_area, hint_area] =
            Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).areas(area);
        let ratio = if pull.total == 0 {
            0.0
        } else {
            pull.completed as f64 / pull.total as f64
        };
        let label = if pull.total == 0 {
            format!("{}  {}", truncate(&pull.label, 24), pull.status)
        } else {
            format!(
                "{}  {}  {} / {}",
                truncate(&pull.label, 20),
                pull.status,
                human_bytes(pull.completed),
                human_bytes(pull.total)
            )
        };
        let gauge = Gauge::default()
            .ratio(ratio.clamp(0.0, 1.0))
            .label(label)
            .gauge_style(Style::default().fg(theme.accent).bg(theme.bg_raised));
        frame.render_widget(gauge, gauge_area);
        draw_hint(frame, app, theme, hint_area, hint);
    } else {
        draw_hint(frame, app, theme, area, hint);
    }
}

fn draw_hint(frame: &mut Frame, app: &App, theme: &Theme, area: Rect, hint: &str) {
    let (label, color) = connection(app, theme);
    let label = format!(" {label} ");
    let label_w = UnicodeWidthStr::width(label.as_str()) + 1;
    let hint_w = (area.width as usize).saturating_sub(label_w);
    let hint = format!(" {}", truncate(hint, hint_w.saturating_sub(1)));
    let hint_w_drawn = UnicodeWidthStr::width(hint.as_str());
    let pad = (area.width as usize).saturating_sub(hint_w_drawn + label_w);
    let bg = theme.bg_deep;
    let line = Line::from(vec![
        Span::styled(hint, Style::default().fg(theme.muted).bg(bg)),
        Span::styled(" ".repeat(pad), Style::default().bg(bg)),
        Span::styled("●", Style::default().fg(color).bg(bg)),
        Span::styled(label, Style::default().fg(theme.fg_dim).bg(bg)),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

fn draw_help(frame: &mut Frame, theme: &Theme, area: Rect) {
    let lines = [
        "Chat",
        "  enter send   alt+enter or ^J newline   esc stops a reply",
        "  wheel, pgup/pgdn, or up/down from the prompt scroll the reply",
        "  ^N new chat   ^M models   ^P system prompt",
        "",
        "Commands, typed in the prompt",
        "  /start  /stop  /restart     Ollama service",
        "  /model [name]               pick the chat model",
        "  /new  /clear  /cancel       new chat, wipe it, stop a reply",
        "  /system [prompt]  /theme [name]  /pull <name>",
        "  /help  /quit",
        "",
        "Settings can start Ollama with the app, and stop it on quit.",
        "tab / shift+tab switch screens    ? help    q or ^C quit",
    ];
    overlay(frame, theme, area, " Help ", &lines, 68, 18);
}

fn draw_confirm(frame: &mut Frame, theme: &Theme, area: Rect, confirm: &crate::app::Confirm) {
    let question = match confirm {
        crate::app::Confirm::DeleteChat { title, .. } => format!("Delete chat \"{title}\"?"),
        crate::app::Confirm::DeleteModel(name) => format!("Delete model {name}?"),
    };
    let text = vec![
        question,
        String::new(),
        "y delete    n or esc cancel".into(),
    ];
    overlay_owned(frame, theme, area, " Confirm ", &text, 56, 7);
}

pub fn overlay(
    frame: &mut Frame,
    theme: &Theme,
    area: Rect,
    title: &str,
    lines: &[&str],
    width: u16,
    height: u16,
) {
    let owned: Vec<String> = lines.iter().map(|s| (*s).to_string()).collect();
    overlay_owned(frame, theme, area, title, &owned, width, height);
}

pub fn overlay_owned(
    frame: &mut Frame,
    theme: &Theme,
    area: Rect,
    title: &str,
    lines: &[String],
    width: u16,
    height: u16,
) {
    let rect = centered(area, width, height);
    frame.render_widget(Clear, rect);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent))
        .title(format!(" {title} "))
        .style(theme.base());
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    let text: Vec<Line> = lines
        .iter()
        .map(|line| Line::from(Span::styled(line.clone(), Style::default().fg(theme.fg))))
        .collect();
    frame.render_widget(Paragraph::new(text), inner);
}

pub fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width).max(1);
    let height = height.min(area.height).max(1);
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    Rect::new(x, y, width, height)
}

pub fn panel(title: &str, focused: bool, theme: &Theme) -> Block<'static> {
    let border = if focused { theme.accent } else { theme.border };
    Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border))
        .title(format!(" {title} "))
        .style(theme.base())
}

pub fn field_paragraph(
    field: &Field,
    width: usize,
    max_lines: usize,
    style: Style,
) -> Paragraph<'static> {
    Paragraph::new(field_lines(field, width, max_lines, style))
}

pub fn field_lines(
    field: &Field,
    width: usize,
    max_lines: usize,
    style: Style,
) -> Vec<Line<'static>> {
    let width = width.max(1);
    let max_lines = max_lines.max(1);
    let mut rows: Vec<Vec<(char, bool)>> = vec![Vec::new()];
    let mut col = 0usize;
    let mut cursor_at = 0usize;
    for (byte, ch) in field.value.char_indices() {
        let cursor = byte == field.cursor;
        if ch == '\n' {
            if cursor {
                rows.last_mut().unwrap().push((' ', true));
                cursor_at = rows.len() - 1;
            }
            rows.push(Vec::new());
            col = 0;
            continue;
        }
        let cw = UnicodeWidthChar::width(ch).unwrap_or(1).max(1);
        if col + cw > width && !rows.last().unwrap().is_empty() {
            rows.push(Vec::new());
            col = 0;
        }
        if cursor {
            cursor_at = rows.len() - 1;
        }
        rows.last_mut().unwrap().push((ch, cursor));
        col += cw;
    }
    if field.cursor >= field.value.len() {
        if col >= width && !rows.last().unwrap().is_empty() {
            rows.push(Vec::new());
        }
        rows.last_mut().unwrap().push((' ', true));
        cursor_at = rows.len() - 1;
    }
    let start = cursor_at.saturating_add(1).saturating_sub(max_lines);
    let cursor_style = Style::default()
        .bg(style.fg.unwrap_or(ratatui::style::Color::White))
        .fg(style.bg.unwrap_or(ratatui::style::Color::Black));
    rows.into_iter()
        .skip(start)
        .take(max_lines)
        .map(|row| {
            let mut spans = Vec::new();
            for (ch, cursor) in row {
                let text = ch.to_string();
                spans.push(Span::styled(
                    text,
                    if cursor { cursor_style } else { style },
                ));
            }
            if spans.is_empty() {
                spans.push(Span::styled(" ", style));
            }
            Line::from(spans)
        })
        .collect()
}

pub fn swatch_line(theme: &Theme) -> Line<'static> {
    let mut spans = Vec::new();
    for color in [
        theme.bg,
        theme.fg,
        theme.accent,
        theme.red,
        theme.green,
        theme.blue,
    ] {
        spans.push(Span::styled("  ", Style::default().bg(color)));
        spans.push(Span::raw(" "));
    }
    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use crate::config::Config;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use tokio::sync::mpsc::unbounded_channel;

    fn buffer_text(terminal: &Terminal<TestBackend>) -> String {
        let buffer = terminal.backend().buffer();
        let mut out = String::new();
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                out.push_str(buffer[(x, y)].symbol());
            }
            out.push('\n');
        }
        out
    }

    #[test]
    fn four_screens_render() {
        let (tx, _rx) = unbounded_channel();
        let mut config = Config::default();
        config.theme = "dracula".into();
        let mut app = App::new(
            config,
            std::path::PathBuf::from("/tmp/ollatui-test-config.toml"),
            Vec::new(),
            None,
            String::new(),
            None,
            tx,
        );
        let mut terminal = Terminal::new(TestBackend::new(100, 32)).unwrap();
        terminal.draw(|f| draw(f, &mut app)).unwrap();
        let chat = buffer_text(&terminal);
        assert!(chat.contains("ollatui"), "{chat}");
        assert!(chat.contains("Chat"), "{chat}");
        assert!(chat.contains("Chats"), "{chat}");

        app.screen = Screen::Models;
        terminal.draw(|f| draw(f, &mut app)).unwrap();
        let models = buffer_text(&terminal);
        assert!(models.contains("Installed"), "{models}");

        app.screen = Screen::Marketplace;
        terminal.draw(|f| draw(f, &mut app)).unwrap();
        let market = buffer_text(&terminal);
        assert!(market.contains("Search"), "{market}");

        app.screen = Screen::Settings;
        terminal.draw(|f| draw(f, &mut app)).unwrap();
        let settings = buffer_text(&terminal);
        assert!(settings.contains("Theme"), "{settings}");
        assert!(settings.contains("Dracula"), "{settings}");
        assert!(settings.contains("Autostart"), "{settings}");
        assert!(!settings.contains("Start Ollama"), "{settings}");
        assert!(chat.contains("connecting"), "{chat}");
        assert!(!chat.contains("Dracula"), "{chat}");

        app.screen = Screen::Chat;
        app.connected = Some(false);
        app.config.host = "http://127.0.0.1:11434".into();
        terminal.draw(|f| draw(f, &mut app)).unwrap();
        let offline = buffer_text(&terminal);
        assert!(offline.contains("Ollama is not running"), "{offline}");
    }
}
