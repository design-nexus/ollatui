use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    List, ListItem, ListState, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
};
use ratatui::Frame;

use crate::app::{App, ChatPane};
use crate::store::Message;
use crate::theme::Theme;
use crate::ui::{self, field_paragraph, panel};
use crate::util::{hard_wrap, split_think, truncate, wrap};

pub fn draw(frame: &mut Frame, app: &mut App, area: Rect) {
    if area.height < 6 || area.width < 10 {
        return;
    }
    let theme = app.theme.clone();
    let text_rows = composer_rows(&app.composer.value, area.width.saturating_sub(2) as usize);
    let composer_h = (text_rows as u16 + 2)
        .min(area.height.saturating_sub(1))
        .max(3);
    let [top, composer_area] =
        Layout::vertical([Constraint::Min(1), Constraint::Length(composer_h)]).areas(area);
    let transcript = if app.show_chats {
        let side_w = if top.width > 64 { 26 } else { 16 };
        let [side, transcript] =
            Layout::horizontal([Constraint::Length(side_w), Constraint::Min(1)]).areas(top);
        draw_sidebar(frame, app, &theme, side);
        transcript
    } else {
        top
    };
    draw_transcript(frame, app, &theme, transcript);
    draw_composer(frame, app, &theme, composer_area);
    if app.model_palette_open() {
        draw_model_palette(frame, app, &theme, transcript);
    } else if app.slash_palette_open() {
        draw_slash_palette(frame, app, &theme, transcript);
    }
}

fn draw_sidebar(frame: &mut Frame, app: &App, theme: &Theme, area: Rect) {
    let focused = app.chat_pane == ChatPane::Sidebar;
    let block = panel("Chats", focused, theme);
    let items: Vec<ListItem> = app
        .chats
        .iter()
        .map(|chat| {
            let marker = if app.streaming_chat.as_deref() == Some(chat.id.as_str()) {
                "● "
            } else {
                "  "
            };
            let title = format!("{marker}{}", chat.title);
            ListItem::new(truncate(&title, area.width.saturating_sub(4) as usize))
        })
        .collect();
    let mut state = ListState::default();
    if !items.is_empty() {
        state.select(Some(app.chat_index));
    }
    frame.render_stateful_widget(
        List::new(items)
            .block(block)
            .highlight_style(theme.selected()),
        area,
        &mut state,
    );
}

fn draw_transcript(frame: &mut Frame, app: &mut App, theme: &Theme, area: Rect) {
    let focused = app.chat_pane == ChatPane::Transcript;
    let title = app
        .chat()
        .map(|c| truncate(&c.title, 40))
        .unwrap_or_else(|| "Chat".into());
    let block = panel(&title, focused, theme);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let width = inner.width as usize;
    let lines = transcript_lines(app, theme, width);
    let view = inner.height as usize;
    app.transcript_view = view.max(1);
    app.transcript_len = lines.len();
    if app.stick_bottom {
        app.scroll = lines.len().saturating_sub(view);
    }
    let max_scroll = lines.len().saturating_sub(view);
    if app.scroll > max_scroll {
        app.scroll = max_scroll;
    }
    let scroll = app.scroll;
    let total = lines.len();
    let visible: Vec<Line> = lines.into_iter().skip(scroll).take(view).collect();
    frame.render_widget(Paragraph::new(visible), inner);
    if total > view {
        let mut state = ScrollbarState::new(total.saturating_sub(view))
            .position(scroll)
            .viewport_content_length(view);
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .style(Style::default().fg(theme.muted)),
            area,
            &mut state,
        );
    }
}

fn transcript_lines(app: &App, theme: &Theme, width: usize) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if app.connected == Some(false) {
        lines.push(Line::from(Span::styled(
            format!("Ollama is not running at {}. Type /start.", app.config.host),
            Style::default().fg(theme.red),
        )));
        lines.push(Line::from(""));
    }
    let Some(chat) = app.chat() else {
        return lines;
    };
    if !chat.system.trim().is_empty() {
        lines.push(Line::from(Span::styled(
            format!(
                "system  {}",
                truncate(&chat.system.replace('\n', " "), width.saturating_sub(8))
            ),
            Style::default()
                .fg(theme.magenta)
                .add_modifier(Modifier::ITALIC),
        )));
        lines.push(Line::from(""));
    }
    if chat.messages.is_empty() {
        lines.push(Line::from(Span::styled(
            "Ask something. Enter sends. Up or the wheel scrolls the reply. /help lists commands.",
            Style::default().fg(theme.muted),
        )));
        return lines;
    }
    for (index, message) in chat.messages.iter().enumerate() {
        let selected = app.chat_pane == ChatPane::Transcript && index == app.transcript_cursor;
        lines.extend(message_lines(
            message,
            theme,
            width,
            selected,
            app.config.show_token_stats,
            app.config.assistant_name.as_str(),
        ));
        lines.push(Line::from(""));
    }
    lines
}

fn message_lines(
    message: &Message,
    theme: &Theme,
    width: usize,
    selected: bool,
    show_stats: bool,
    assistant_name: &str,
) -> Vec<Line<'static>> {
    if message.role == "user" {
        return user_message_lines(message, theme, width, selected);
    }
    let mut lines = Vec::new();
    let name = assistant_name.trim();
    let name = if name.is_empty() { "assistant" } else { name };
    let marker = if selected { "▌ " } else { "  " };
    lines.push(Line::from(Span::styled(
        format!("{marker}{name}"),
        Style::default()
            .fg(theme.green)
            .add_modifier(Modifier::BOLD),
    )));

    let (tagged, body) = split_think(&message.content);
    let mut thinking = message.thinking.clone();
    if !tagged.trim().is_empty() {
        if !thinking.is_empty() {
            thinking.push('\n');
        }
        thinking.push_str(&tagged);
    }
    let thinking = thinking.trim().to_string();
    if !thinking.is_empty() {
        let label = if message.thinking_open {
            "▾ thinking"
        } else {
            "▸ thinking"
        };
        lines.push(Line::from(Span::styled(
            format!("  {label}"),
            Style::default()
                .fg(theme.muted)
                .add_modifier(Modifier::ITALIC),
        )));
        if message.thinking_open {
            for line in wrap(&thinking, width.saturating_sub(2)) {
                lines.push(Line::from(Span::styled(
                    format!("  {line}"),
                    Style::default()
                        .fg(theme.fg_dim)
                        .add_modifier(Modifier::ITALIC),
                )));
            }
        }
    }

    let mut in_code = false;
    let mut code = String::new();
    let flush_code = |code: &mut String, lines: &mut Vec<Line<'static>>| {
        if code.is_empty() {
            return;
        }
        for line in hard_wrap(code, width.saturating_sub(2)) {
            lines.push(Line::from(Span::styled(
                format!(" {line}"),
                Style::default().fg(theme.cyan).bg(theme.bg_raised),
            )));
        }
        code.clear();
    };
    for line in body.split('\n') {
        if line.trim_start().starts_with("```") {
            if in_code {
                flush_code(&mut code, &mut lines);
                in_code = false;
            } else {
                in_code = true;
            }
            continue;
        }
        if in_code {
            if !code.is_empty() {
                code.push('\n');
            }
            code.push_str(line);
        } else if line.is_empty() {
            lines.push(Line::from(""));
        } else {
            for wrapped in wrap(line, width.saturating_sub(2)) {
                lines.push(Line::from(Span::styled(
                    format!("  {wrapped}"),
                    Style::default().fg(theme.fg),
                )));
            }
        }
    }
    if in_code {
        flush_code(&mut code, &mut lines);
    }
    if show_stats {
        if let Some(stats) = &message.stats {
            lines.push(Line::from(Span::styled(
                format!("  {:.1} tok/s · {} tok", stats.tokens_per_sec, stats.tokens),
                Style::default().fg(theme.muted),
            )));
        }
    }
    lines
}

fn user_message_lines(
    message: &Message,
    theme: &Theme,
    width: usize,
    _selected: bool,
) -> Vec<Line<'static>> {
    let bg = lift(theme.bg);
    let text_style = Style::default().fg(theme.fg).bg(bg);
    let prompt_style = Style::default()
        .fg(theme.green)
        .bg(bg)
        .add_modifier(Modifier::BOLD);
    let prompt = "❯ ";
    let prompt_cols = crate::util::width_of(prompt).max(1);
    let text_cols = width.saturating_sub(prompt_cols).max(1);
    let mut lines = Vec::new();
    let body = message.content.trim();
    if body.is_empty() || width == 0 {
        lines.push(user_line(prompt, prompt_style, "", text_style, text_cols));
        return lines;
    }
    let mut first = true;
    for para in body.split('\n') {
        if para.is_empty() {
            lines.push(user_line(
                &" ".repeat(prompt_cols),
                text_style,
                "",
                text_style,
                text_cols,
            ));
            continue;
        }
        for wrapped in wrap(para, text_cols) {
            if first {
                lines.push(user_line(
                    prompt,
                    prompt_style,
                    &wrapped,
                    text_style,
                    text_cols,
                ));
                first = false;
            } else {
                lines.push(user_line(
                    &" ".repeat(prompt_cols),
                    text_style,
                    &wrapped,
                    text_style,
                    text_cols,
                ));
            }
        }
    }
    lines
}

fn user_line(
    prefix: &str,
    prefix_style: Style,
    text: &str,
    text_style: Style,
    text_cols: usize,
) -> Line<'static> {
    let shown = if crate::util::width_of(text) > text_cols {
        truncate(text, text_cols)
    } else {
        text.to_string()
    };
    let pad = text_cols.saturating_sub(crate::util::width_of(&shown));
    Line::from(vec![
        Span::styled(prefix.to_string(), prefix_style),
        Span::styled(shown, text_style),
        Span::styled(" ".repeat(pad), text_style),
    ])
}

fn lift(color: Color) -> Color {
    match color {
        Color::Rgb(r, g, b) => Color::Rgb(bump(r), bump(g), bump(b)),
        other => other,
    }
}

fn bump(channel: u8) -> u8 {
    channel.saturating_add(18)
}

fn composer_rows(value: &str, width: usize) -> usize {
    let width = width.max(1);
    let mut rows = 0usize;
    let parts: Vec<&str> = value.split('\n').collect();
    for (index, part) in parts.iter().enumerate() {
        let cols = crate::util::width_of(part);
        let cursor = usize::from(index + 1 == parts.len());
        rows += (cols + cursor).div_ceil(width).max(1);
    }
    rows.clamp(1, 8)
}

fn draw_model_palette(frame: &mut Frame, app: &App, theme: &Theme, area: Rect) {
    let matches = app.model_matches();
    let shown = 8usize;
    let pick = if matches.is_empty() {
        0
    } else {
        app.slash_pick.min(matches.len() - 1)
    };
    let start = pick
        .saturating_add(1)
        .saturating_sub(shown)
        .min(matches.len().saturating_sub(shown));
    let window: Vec<_> = matches.iter().skip(start).take(shown).collect();
    let rows = window.len().max(1) as u16;
    let height = rows.saturating_add(2).min(area.height);
    let width = 52.min(area.width);
    let rect = Rect::new(
        area.x,
        area.y + area.height.saturating_sub(height),
        width,
        height,
    );
    frame.render_widget(ratatui::widgets::Clear, rect);
    let block = panel("Models", true, theme);
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    if inner.height == 0 {
        return;
    }
    let mut lines = Vec::new();
    if window.is_empty() {
        lines.push(Line::from(Span::styled(
            "No installed models",
            Style::default().fg(theme.muted),
        )));
    }
    for (offset, name) in window.iter().enumerate() {
        let selected = start + offset == pick;
        let style = if selected {
            theme.selected()
        } else {
            Style::default().fg(theme.fg).bg(theme.bg)
        };
        lines.push(Line::from(Span::styled(
            truncate(name, inner.width as usize),
            style,
        )));
    }
    frame.render_widget(Paragraph::new(lines), inner);
}

fn draw_slash_palette(frame: &mut Frame, app: &App, theme: &Theme, area: Rect) {
    let matches = app.slash_matches();
    let shown = 8usize;
    let pick = if matches.is_empty() {
        0
    } else {
        app.slash_pick.min(matches.len() - 1)
    };
    let start = pick
        .saturating_add(1)
        .saturating_sub(shown)
        .min(matches.len().saturating_sub(shown));
    let window: Vec<_> = if matches.is_empty() {
        Vec::new()
    } else {
        matches.iter().skip(start).take(shown).collect()
    };
    let rows = window.len().max(1) as u16;
    let height = rows.saturating_add(2).min(area.height);
    let width = 52.min(area.width);
    let rect = Rect::new(
        area.x,
        area.y + area.height.saturating_sub(height),
        width,
        height,
    );
    frame.render_widget(ratatui::widgets::Clear, rect);
    let block = panel("Commands", true, theme);
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    if inner.height == 0 {
        return;
    }
    let mut lines = Vec::new();
    if window.is_empty() {
        lines.push(Line::from(Span::styled(
            "No matching commands",
            Style::default().fg(theme.muted),
        )));
    }
    for (offset, command) in window.iter().enumerate() {
        let selected = start + offset == pick;
        let style = if selected {
            theme.selected()
        } else {
            Style::default().fg(theme.fg).bg(theme.bg)
        };
        let name = format!("/{:<8}", command.name);
        let summary = truncate(command.summary, inner.width.saturating_sub(12) as usize);
        lines.push(Line::from(vec![
            Span::styled(
                name,
                if selected {
                    style
                } else {
                    Style::default().fg(theme.accent).bg(theme.bg)
                },
            ),
            Span::styled(summary, style),
        ]));
    }
    frame.render_widget(Paragraph::new(lines), inner);
}

fn draw_composer(frame: &mut Frame, app: &App, theme: &Theme, area: Rect) {
    let focused = app.chat_pane == ChatPane::Composer
        && !app.model_picker
        && !app.renaming
        && !app.editing_system;
    let block = panel(
        if app.streaming {
            "Generating"
        } else {
            "Prompt"
        },
        focused,
        theme,
    );
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let style = Style::default().fg(theme.fg).bg(theme.bg);
    let paragraph = if focused {
        field_paragraph(
            &app.composer,
            inner.width as usize,
            inner.height as usize,
            style,
        )
    } else {
        Paragraph::new(app.composer.value.clone())
    };
    frame.render_widget(paragraph, inner);
}

pub fn draw_model_picker(frame: &mut Frame, app: &App, area: Rect) {
    let theme = &app.theme;
    let rect = ui::centered(area, 56, 16);
    frame.render_widget(ratatui::widgets::Clear, rect);
    let block = panel("Models", true, theme);
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    if inner.height < 3 {
        return;
    }
    let [filter_area, list_area] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(inner);
    let style = Style::default().fg(theme.fg).bg(theme.bg);
    frame.render_widget(
        field_paragraph(&app.model_filter, filter_area.width as usize, 1, style),
        filter_area,
    );
    let choices = app.model_choices();
    let items: Vec<ListItem> = if choices.is_empty() {
        vec![ListItem::new("No installed models")]
    } else {
        choices
            .into_iter()
            .map(|name| ListItem::new(name))
            .collect()
    };
    let mut state = ListState::default();
    if !app.model_choices().is_empty() {
        state.select(Some(app.model_pick.min(items.len().saturating_sub(1))));
    }
    frame.render_stateful_widget(
        List::new(items).highlight_style(theme.selected()),
        list_area,
        &mut state,
    );
}

pub fn draw_rename(frame: &mut Frame, app: &App, area: Rect) {
    let theme = &app.theme;
    let rect = ui::centered(area, 50, 5);
    frame.render_widget(ratatui::widgets::Clear, rect);
    let block = panel("Rename", true, theme);
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    if inner.width > 0 && inner.height > 0 {
        let style = Style::default().fg(theme.fg).bg(theme.bg);
        frame.render_widget(
            field_paragraph(&app.rename_field, inner.width as usize, 1, style),
            inner,
        );
    }
}

pub fn draw_system(frame: &mut Frame, app: &App, area: Rect) {
    let theme = &app.theme;
    let rect = ui::centered(area, 64, 12);
    frame.render_widget(ratatui::widgets::Clear, rect);
    let block = panel("System prompt  ·  ^S save  esc cancel", true, theme);
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    if inner.width > 0 && inner.height > 0 {
        let style = Style::default().fg(theme.fg).bg(theme.bg);
        frame.render_widget(
            field_paragraph(
                &app.system_field,
                inner.width as usize,
                inner.height as usize,
                style,
            ),
            inner,
        );
    }
}
