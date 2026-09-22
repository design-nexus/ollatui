use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use crate::app::{App, Setting};
use crate::theme::{self, Theme};
use crate::ui::{field_paragraph, panel, swatch_line};
use crate::util::truncate;

pub fn draw(frame: &mut Frame, app: &App, area: Rect) {
    if area.width < 10 || area.height < 4 {
        return;
    }
    let theme = app.theme.clone();
    let [list_area, preview] =
        Layout::horizontal([Constraint::Percentage(58), Constraint::Percentage(42)]).areas(area);
    if app.picking_theme {
        draw_themes(frame, app, &theme, list_area);
    } else {
        draw_settings(frame, app, &theme, list_area);
    }
    draw_preview(frame, app, &theme, preview);
}

fn draw_settings(frame: &mut Frame, app: &App, theme: &Theme, area: Rect) {
    let block = panel("Settings", !app.editing, theme);
    let items: Vec<ListItem> = Setting::ALL
        .iter()
        .enumerate()
        .map(|(index, setting)| {
            let value = if app.editing && index == app.settings_index {
                app.edit_field.value.clone()
            } else {
                app.setting_value(*setting)
            };
            let label = format!("{:<18} {}", setting.label(), truncate(&value, 28));
            ListItem::new(label)
        })
        .collect();
    let mut state = ListState::default();
    state.select(Some(app.settings_index));
    frame.render_stateful_widget(
        List::new(items)
            .block(block)
            .highlight_style(theme.selected()),
        area,
        &mut state,
    );
}

fn draw_themes(frame: &mut Frame, app: &App, theme: &Theme, area: Rect) {
    let block = panel("Themes", true, theme);
    let mut items = Vec::new();
    let mut last_group = "";
    for choice in theme::CHOICES {
        if choice.group != last_group {
            items.push(ListItem::new(Span::styled(
                choice.group,
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            )));
            last_group = choice.group;
        }
        let mut label = choice.label.to_string();
        if choice.id == "omarchy" && app.omarchy.is_none() {
            label.push_str(" (not installed)");
        }
        if choice.id == app.config.theme {
            label.push_str("  ·");
        }
        items.push(ListItem::new(label));
    }
    // The highlight index counts every row, including headers, so map the
    // choice index onto the rendered row.
    let mut state = ListState::default();
    state.select(Some(rendered_row(app.theme_index)));
    frame.render_stateful_widget(
        List::new(items)
            .block(block)
            .highlight_style(theme.selected()),
        area,
        &mut state,
    );
}

fn rendered_row(choice_index: usize) -> usize {
    let mut row = 0;
    let mut last = "";
    for (index, choice) in theme::CHOICES.iter().enumerate() {
        if choice.group != last {
            row += 1;
            last = choice.group;
        }
        if index == choice_index {
            return row;
        }
        row += 1;
    }
    0
}

fn draw_preview(frame: &mut Frame, app: &App, theme: &Theme, area: Rect) {
    let preview = if app.picking_theme {
        theme::CHOICES
            .get(app.theme_index)
            .map(|c| app.preview_theme(c.id))
            .unwrap_or_else(|| theme.clone())
    } else {
        theme.clone()
    };
    let title = if app.picking_theme {
        preview.name.as_str()
    } else {
        "Preview"
    };
    let block = panel(title, false, theme);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let setting = Setting::ALL[app.settings_index];
    let mut lines = vec![
        swatch_line(&preview),
        Line::from(""),
        Line::from(Span::styled(
            preview.name.clone(),
            Style::default()
                .fg(preview.accent)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            "The quick brown fox",
            Style::default().fg(preview.fg).bg(preview.bg),
        )),
        Line::from(""),
    ];
    let mut field_row = None;
    if app.editing {
        lines.push(Line::from(Span::styled(
            setting.label(),
            Style::default().fg(theme.accent),
        )));
        field_row = Some(lines.len() as u16);
        lines.push(Line::from(""));
        lines.push(Line::from("Enter saves. Esc cancels."));
    } else if !app.picking_theme {
        lines.push(Line::from(Span::styled(
            setting.label(),
            Style::default().fg(theme.accent),
        )));
        lines.push(Line::from(setting.hint()));
    } else if preview.id == "omarchy" && app.omarchy.is_none() {
        lines.push(Line::from("Omarchy was not found on this machine."));
        lines.push(Line::from("Install it, or pick another theme."));
    } else {
        lines.push(Line::from("Enter applies this theme."));
        if preview.id == "omarchy" {
            lines.push(Line::from("It follows the live Omarchy palette."));
        }
    }
    frame.render_widget(Paragraph::new(lines), inner);
    if let Some(row) = field_row {
        if row < inner.height {
            let field_area = Rect::new(inner.x, inner.y + row, inner.width, 1);
            let style = Style::default().fg(theme.fg).bg(theme.bg);
            frame.render_widget(
                field_paragraph(&app.edit_field, field_area.width as usize, 1, style),
                field_area,
            );
        }
    }
}
