use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, BorderType, Borders, ListItem, Paragraph, Table, Row, Wrap},
    Frame,
};

use crate::app::{App, InputField, JumpField, Screen};
use crate::ssh::format_command;

pub fn draw(frame: &mut Frame, app: &mut App) {
    let status_text = app
        .status
        .clone()
        .unwrap_or_else(|| format!("{} tunnel(s) configured", app.config.tunnels.len()));

    let chunks = Layout::new(Direction::Vertical, [
        Constraint::Min(0),
        Constraint::Length(3),
    ])
    .margin(1)
    .split(frame.area());

    match app.screen {
        Screen::List => {
            let list_chunks = Layout::new(Direction::Horizontal, [
                Constraint::Min(24),
                Constraint::Length(46),
            ])
            .split(chunks[0]);
            draw_list(frame, app, list_chunks[0]);
            draw_details(frame, app, list_chunks[1]);
            draw_list_hints(frame, chunks[1], &status_text);
        }
        Screen::Create | Screen::Edit => {
            draw_form(frame, app, chunks[0]);
            draw_status_bar(frame, chunks[1], &status_text);
        }
        Screen::JumpAdd | Screen::JumpEdit => {
            draw_jump_form(frame, app, chunks[0]);
            draw_status_bar(frame, chunks[1], &status_text);
        }
        Screen::Folder => {
            let list_chunks = Layout::new(Direction::Horizontal, [
                Constraint::Min(24),
                Constraint::Length(46),
            ])
            .split(chunks[0]);
            draw_list(frame, app, list_chunks[0]);
            draw_details(frame, app, list_chunks[1]);
            draw_list_hints(frame, chunks[1], &status_text);
            draw_folder_picker(frame, app, chunks[0]);
        }
    }
}

fn draw_details(frame: &mut Frame, app: &mut App, area: Rect) {
    let block = Block::default()
        .title(" Selected tunnel ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let Some(idx) = app.selected_tunnel() else {
        let p = Paragraph::new("Nothing selected.")
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(p, inner);
        return;
    };
    let tunnel = &app.config.tunnels[idx];
    let cmd = format_command(tunnel);
    let output = app
        .ssh
        .output(&tunnel.name)
        .map(str::to_string)
        .unwrap_or_else(|| "(no output yet)".to_string());
    let jumps: String = if tunnel.jumps.is_empty() {
        "(direct)".to_string()
    } else {
        tunnel
            .jumps
            .iter()
            .map(|j| {
                let user = if j.user.is_empty() {
                    String::new()
                } else {
                    format!("{}@", j.user)
                };
                format!("{}{}:{}", user, j.host, j.port)
            })
            .collect::<Vec<_>>()
            .join(" -> ")
    };
    let content = format!(
        "Jumps: {}\nCommand: {}\nOutput: {}",
        jumps, cmd, output
    );
    let p = Paragraph::new(content).wrap(Wrap { trim: false });
    frame.render_widget(p, inner);
}

fn draw_list_hints(frame: &mut Frame, area: Rect, status: &str) {
    let hints = Paragraph::new(format!(
        "Enter: run/stop or collapse folder | j/k: nav | /: search | n: new | c: duplicate | e: edit | d: delete | f: folder | q: quit\n{}",
        status
    ))
    .style(Style::default().fg(Color::DarkGray))
    .alignment(Alignment::Center);
    frame.render_widget(hints, area);
}

fn draw_folder_picker(frame: &mut Frame, app: &App, area: Rect) {
    let Some(picker) = &app.folder_picker else {
        return;
    };
    let title = if picker.editing.is_some() {
        match picker.editing {
            Some(crate::app::EditKind::Create) => " New folder ",
            Some(crate::app::EditKind::Rename(_)) => " Rename folder ",
            None => " Folders ",
        }
    } else {
        " Folders "
    };
    let popup_area = centered_rect(area, 40, 46);
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title_alignment(Alignment::Center);
    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    if let Some(crate::app::EditKind::Create) | Some(crate::app::EditKind::Rename(_)) =
        picker.editing
    {
        let label = match picker.editing {
            Some(crate::app::EditKind::Create) => "Name",
            Some(crate::app::EditKind::Rename(_)) => "New name",
            None => "",
        };
        let mut text = format!("{}: {}_\n", label, picker.new_name);
        if let Some(err) = &picker.error {
            text.push_str(&format!("Error: {err}"));
        }
        let para = Paragraph::new(text).style(Style::default().fg(Color::Yellow));
        frame.render_widget(para, Rect {
            x: inner.x,
            y: inner.y,
            width: inner.width,
            height: inner.height,
        });
        return;
    }

    let mut items: Vec<ListItem> = Vec::new();
    let entries: Vec<String> = {
        let mut v = vec!["(none)".to_string()];
        v.extend(picker.folders.iter().cloned());
        v.push("(new…)".to_string());
        v
    };
    for (i, entry) in entries.iter().enumerate() {
        let style = if i == picker.selected {
            Style::default().bg(Color::Blue).fg(Color::White)
        } else {
            Style::default()
        };
        let suffix = match i {
            0 => String::new(),
            n if n == picker.folders.len() + 1 => String::new(),
            n => {
                let count = app
                    .config
                    .tunnels
                    .iter()
                    .filter(|t| t.folder.trim() == picker.folders[n - 1])
                    .count();
                format!(" ({count})")
            }
        };
        items.push(ListItem::new(format!("{}{}", entry, suffix)).style(style));
    }
    let list = ratatui::widgets::List::new(items);
    frame.render_widget(list, inner);

    let hint = Paragraph::new("Enter: assign | r: rename | x: delete | Esc: back")
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center);
    frame.render_widget(hint, Rect {
        x: inner.x,
        y: inner.y + inner.height - 1,
        width: inner.width,
        height: 1,
    });
}

fn centered_rect(area: Rect, height: u16, width: u16) -> Rect {
    let vertical = Layout::new(Direction::Vertical, [
        Constraint::Percentage((100 - height) / 2),
        Constraint::Length(height),
        Constraint::Percentage((100 - height) / 2),
    ])
    .split(area);
    let horizontal = Layout::new(Direction::Horizontal, [
        Constraint::Percentage((100 - width) / 2),
        Constraint::Length(width),
        Constraint::Percentage((100 - width) / 2),
    ])
    .split(vertical[1]);
    horizontal[1]
}

fn draw_status_bar(frame: &mut Frame, area: Rect, text: &str) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded);
    let para = Paragraph::new(text).block(block).style(Style::default());
    frame.render_widget(para, area);
}

fn draw_list(frame: &mut Frame, app: &mut App, area: Rect) {
    let search_text = if app.search_mode {
        format!("/{}_", app.search)
    } else if !app.search.is_empty() {
        format!("(filtered by: {})", app.search)
    } else {
        String::new()
    };
    let title = if search_text.is_empty() {
        " SSH Tunnel Manager ".to_string()
    } else {
        format!(" SSH Tunnel Manager {} ", search_text)
    };
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title_alignment(Alignment::Left);

    let rows = app.visible_rows();
    let tunnel_names: Vec<String> = rows
        .iter()
        .filter_map(|r| match r {
            crate::app::ListRow::Tunnel(i) => Some(app.config.tunnels[*i].name.clone()),
            crate::app::ListRow::Header(_) => None,
        })
        .collect();
    let statuses = app.ssh.running_snapshot(&tunnel_names.iter().map(|s| s.as_str()).collect::<Vec<_>>());

    let mut ord = 1;
    let mut rows_widgets: Vec<Row> = Vec::new();
    for (shown_idx, row) in rows.iter().enumerate() {
        match row {
            crate::app::ListRow::Header(folder) => {
                let header_style = if shown_idx == app.list.selected {
                    Style::default()
                        .bg(Color::DarkGray)
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                        .fg(Color::DarkGray)
                        .add_modifier(Modifier::BOLD)
                };
                let collapsed = app.collapsed.contains(folder);
                let count = app
                    .config
                    .tunnels
                    .iter()
                    .filter(|t| t.folder.trim() == *folder)
                    .count();
                let arrow = if collapsed { "▸" } else { "▾" };
                let label = format!(" {} {} ({})", arrow, folder, count);
                rows_widgets.push(
                    Row::new(vec![
                        String::new(),
                        label,
                        String::new(),
                        String::new(),
                        String::new(),
                        String::new(),
                    ])
                    .style(header_style),
                );
            }
            crate::app::ListRow::Tunnel(real_idx) => {
                let t = &app.config.tunnels[*real_idx];
                let style = if shown_idx == app.list.selected {
                    Style::default()
                        .bg(Color::Blue)
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                let name = &t.name;
                let running = statuses
                    .iter()
                    .find(|(n, _)| n == name)
                    .map(|(_, r)| *r)
                    .unwrap_or(false);
                let status = if running { "RUNNING" } else { "STOPPED" };
                let is_target = format!("{}:{}", t.target.host, t.target.port);
                let jumps = t.jumps.len().to_string();
                rows_widgets.push(
                    Row::new(vec![
                        ord.to_string(),
                        t.name.clone(),
                        status.to_string(),
                        t.local_port.to_string(),
                        is_target,
                        jumps,
                    ])
                    .style(style),
                );
                ord += 1;
            }
        }
    }

    let header = Row::new(vec!["#", "Name", "Status", "Local Port", "Target", "Jumps"])
        .style(Style::default().add_modifier(Modifier::BOLD));

    let table = Table::new(rows_widgets, [
        Constraint::Length(3),
        Constraint::Length(20),
        Constraint::Length(9),
        Constraint::Length(10),
        Constraint::Length(30),
        Constraint::Length(6),
    ])
    .header(header)
    .block(block);

    frame.render_widget(table, area);
}

fn draw_form(frame: &mut Frame, app: &App, area: Rect) {
    let form = &app.form;
    let title = if app.screen == Screen::Edit {
        " Edit Tunnel "
    } else {
        " New Tunnel "
    };
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title_alignment(Alignment::Center);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::new(Direction::Vertical, [
        Constraint::Length(5),
        Constraint::Min(4),
        Constraint::Length(9),
    ])
    .split(inner);

    // Top fields
    let top = Layout::new(Direction::Horizontal, [
        Constraint::Percentage(50),
        Constraint::Percentage(50),
    ])
    .split(chunks[0]);

    draw_input(
        frame,
        top[0],
        "Name",
        &form.name,
        form.input == InputField::Name,
    );
    draw_input(
        frame,
        top[1],
        "Local Port",
        &form.local_port,
        form.input == InputField::LocalPort,
    );

    // Jumps list
    let jumps_title = format!(
        " Jump Hosts [a=add, e=edit, x=remove, j/k=move] ",
    );
    let jump_style = if form.input == InputField::Jumps {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let jumps_block = Block::default()
        .title(jumps_title)
        .borders(Borders::ALL)
        .border_style(jump_style);
    let jumps_inner = jumps_block.inner(chunks[1]);
    frame.render_widget(jumps_block, chunks[1]);

    let items: Vec<ListItem> = form
        .jumps
        .iter()
        .enumerate()
        .map(|(i, j)| {
            let user = if j.user.is_empty() {
                String::new()
            } else {
                format!("{}@", j.user)
            };
            let pwd = if j.password.as_ref().map(|p| !p.is_empty()).unwrap_or(false) {
                " [pwd]"
            } else {
                ""
            };
            ListItem::new(format!("  {} {}{}:{}{}", i + 1, user, j.host, j.port, pwd)).style(if i == form.selected_jump {
                Style::default()
                    .bg(Color::DarkGray)
                    .fg(Color::White)
            } else {
                Style::default()
            })
        })
        .collect();

    if items.is_empty() {
        let empty = Paragraph::new("No jump hosts yet. Press 'a' while here to add one.")
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(empty, jumps_inner);
    } else {
        let list = ratatui::widgets::List::new(items).highlight_style(
            Style::default().bg(Color::Blue).fg(Color::White),
        );
        frame.render_widget(list, jumps_inner);
    }

    // Bottom target fields
    let bottom = Layout::new(Direction::Vertical, [
        Constraint::Length(3),
        Constraint::Length(3),
        Constraint::Length(2),
    ])
    .split(chunks[2]);

    let target_top = Layout::new(Direction::Horizontal, [
        Constraint::Percentage(60),
        Constraint::Percentage(40),
    ])
    .split(bottom[0]);

    draw_input(
        frame,
        target_top[0],
        "Target Host",
        &form.target_host,
        form.input == InputField::TargetHost,
    );
    draw_input(
        frame,
        target_top[1],
        "Target Port",
        &form.target_port,
        form.input == InputField::TargetPort,
    );
    draw_input_masked(
        frame,
        bottom[1],
        "Target Password (optional)",
        &form.target_password,
        form.input == InputField::TargetPassword,
    );

    let legacy_focused = form.input == InputField::Legacy;
    let legacy_style = if legacy_focused {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else if form.legacy {
        Style::default().fg(Color::Green)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let marker = if form.legacy { "[x]" } else { "[ ]" };
    let legacy_text = format!(
        " {} Legacy algorithms (ssh-rsa/ssh-dss){}",
        marker,
        if legacy_focused { "  - press any key to toggle" } else { "" }
    );
    let legacy_para = Paragraph::new(legacy_text).style(legacy_style);
    frame.render_widget(legacy_para, bottom[2]);

    if let Some(err) = &form.error {
        let err_text = format!("Error: {}", err);
        let e = Paragraph::new(err_text)
            .style(Style::default().fg(Color::Red));
        frame.render_widget(e, Rect {
            x: 0,
            y: area.y,
            width: area.width,
            height: 1,
        });
    }
}

fn draw_jump_form(frame: &mut Frame, app: &App, area: Rect) {
    let form = &app.jump_form;
    let title = if app.screen == Screen::JumpEdit {
        " Edit Jump Host "
    } else {
        " Add Jump Host "
    };
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title_alignment(Alignment::Center);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::new(Direction::Vertical, [
        Constraint::Length(3),
        Constraint::Length(3),
        Constraint::Length(3),
        Constraint::Length(3),
        Constraint::Min(1),
    ])
    .split(inner);

    draw_input(frame, chunks[0], "User (optional)", &form.user, form.input == JumpField::User);
    draw_input(frame, chunks[1], "Host", &form.host, form.input == JumpField::Host);
    draw_input(frame, chunks[2], "Port (22)", &form.port, form.input == JumpField::Port);
    draw_input_masked(
        frame,
        chunks[3],
        "Password (optional)",
        &form.password,
        form.input == JumpField::Password,
    );

    if let Some(err) = &form.error {
        let err_text = format!("Error: {}", err);
        let e = Paragraph::new(err_text)
            .style(Style::default().fg(Color::Red));
        frame.render_widget(e, Rect {
            x: area.x,
            y: area.y + 1,
            width: area.width,
            height: 1,
        });
    }
}

fn draw_input_masked(frame: &mut Frame, area: Rect, label: &str, value: &str, focused: bool) {
    let masked: String = "*".repeat(value.chars().count());
    draw_input(frame, area, label, &masked, focused)
}

fn draw_input(frame: &mut Frame, area: Rect, label: &str, value: &str, focused: bool) {
    let border_style = if focused {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let block = Block::default()
        .title(label)
        .borders(Borders::ALL)
        .border_style(border_style);
    let para = Paragraph::new(value).block(block).wrap(Wrap { trim: true });
    frame.render_widget(para, area);
}
