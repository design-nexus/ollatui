use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use crate::app::{App, ModelsPane};
use crate::theme::Theme;
use crate::ui::{field_paragraph, panel};
use crate::util::{human_bytes, truncate};

pub fn draw(frame: &mut Frame, app: &App, area: Rect) {
    if area.height < 4 || area.width < 10 {
        return;
    }
    let theme = app.theme.clone();
    let [body, pull_area] =
        Layout::vertical([Constraint::Min(3), Constraint::Length(3)]).areas(area);
    let [installed, running] =
        Layout::horizontal([Constraint::Percentage(62), Constraint::Percentage(38)]).areas(body);
    draw_installed(frame, app, &theme, installed);
    draw_running(frame, app, &theme, running);
    draw_pull(frame, app, &theme, pull_area);
}

fn draw_installed(frame: &mut Frame, app: &App, theme: &Theme, area: Rect) {
    let block = panel("Installed", app.models_pane == ModelsPane::List, theme);
    let items: Vec<ListItem> = if app.models.is_empty() {
        vec![ListItem::new(if app.connected == Some(false) {
            "Ollama is offline"
        } else {
            "No models yet. Pull one below."
        })]
    } else {
        app.models
            .iter()
            .map(|model| {
                let loaded = app.running.iter().any(|r| r.name == model.name);
                let mark = if loaded { "●" } else { " " };
                let meta = format!(
                    "{} {} {}",
                    model.family, model.parameter_size, model.quantization
                );
                let label = format!(
                    "{mark} {}  {}  {}",
                    truncate(&model.name, 24),
                    human_bytes(model.size),
                    meta.trim()
                );
                ListItem::new(label)
            })
            .collect()
    };
    let mut state = ListState::default();
    if !app.models.is_empty() {
        state.select(Some(app.model_index));
    }
    frame.render_stateful_widget(
        List::new(items)
            .block(block)
            .highlight_style(theme.selected()),
        area,
        &mut state,
    );
}

fn draw_running(frame: &mut Frame, app: &App, theme: &Theme, area: Rect) {
    let block = panel("Running", false, theme);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let mut lines = Vec::new();
    if app.running.is_empty() {
        lines.push(Line::from(Span::styled(
            "Nothing loaded",
            Style::default().fg(theme.muted),
        )));
    } else {
        for model in &app.running {
            lines.push(Line::from(Span::styled(
                truncate(&model.name, inner.width as usize),
                Style::default().fg(theme.fg).add_modifier(Modifier::BOLD),
            )));
            let until = if model.expires.is_empty() {
                human_bytes(model.size)
            } else {
                format!(
                    "{}  until {}",
                    human_bytes(model.size),
                    truncate(&model.expires, 20)
                )
            };
            lines.push(Line::from(Span::styled(
                format!("  {until}"),
                Style::default().fg(theme.muted),
            )));
        }
    }
    if let Some(detail) = &app.detail {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Selected",
            Style::default().fg(theme.accent),
        )));
        if let Some(model) = app.models.get(app.model_index) {
            if !model.modified.is_empty() {
                lines.push(Line::from(truncate(&model.modified, 20)));
            }
        }
        if !detail.family.is_empty() || !detail.parameter_size.is_empty() {
            lines.push(Line::from(
                format!("{} {}", detail.family, detail.parameter_size)
                    .trim()
                    .to_string(),
            ));
        }
        if let Some(ctx) = detail.context_length {
            lines.push(Line::from(format!("context {ctx}")));
        }
        if !detail.quantization.is_empty() {
            lines.push(Line::from(detail.quantization.clone()));
        }
    }
    frame.render_widget(Paragraph::new(lines), inner);
}

fn draw_pull(frame: &mut Frame, app: &App, theme: &Theme, area: Rect) {
    let focused = app.models_pane == ModelsPane::Pull;
    let block = panel("Pull by name", focused, theme);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let style = Style::default().fg(theme.fg).bg(theme.bg);
    let paragraph = if focused {
        field_paragraph(&app.pull_field, inner.width as usize, 1, style)
    } else if app.pull_field.value.is_empty() {
        Paragraph::new(Span::styled(
            "llama3.2   or   hf.co/user/repo:file.gguf",
            Style::default().fg(theme.muted),
        ))
    } else {
        Paragraph::new(app.pull_field.value.clone())
    };
    frame.render_widget(paragraph, inner);
}
