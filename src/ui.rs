use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, BorderType, Borders, ListItem, Paragraph, Table, Row, Wrap},
    Frame,
};

use crate::app::{App, InputField, JumpField, Screen};

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
            draw_list(frame, app, chunks[0]);
            draw_list_hints(frame, chunks[1]);
        }
        Screen::Create | Screen::Edit => {
            draw_form(frame, app, chunks[0]);
            draw_status_bar(frame, chunks[1], &status_text);
        }
        Screen::JumpAdd | Screen::JumpEdit => {
            draw_jump_form(frame, app, chunks[0]);
            draw_status_bar(frame, chunks[1], &status_text);
        }
    }
}

fn draw_list_hints(frame: &mut Frame, area: Rect) {
    let hints = Paragraph::new(
        "Enter: run/stop | n: new | e: edit | d: delete | q: quit",
    )
    .style(Style::default().fg(Color::DarkGray))
    .alignment(Alignment::Center);
    frame.render_widget(hints, area);
}

fn draw_status_bar(frame: &mut Frame, area: Rect, text: &str) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded);
    let para = Paragraph::new(text).block(block).style(Style::default());
    frame.render_widget(para, area);
}

fn draw_list(frame: &mut Frame, app: &mut App, area: Rect) {
    let title = " SSH Tunnel Manager ";
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title_alignment(Alignment::Center);

    let names: Vec<&str> = app.config.tunnels.iter().map(|t| t.name.as_str()).collect();
    let statuses = app.ssh.running_snapshot(&names);

    let rows_widgets: Vec<Row> = app
        .config
        .tunnels
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let style = if i == app.list.selected {
                Style::default()
                    .bg(Color::Blue)
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            let running = statuses
                .iter()
                .find(|(name, _)| name == &t.name)
                .map(|(_, r)| *r)
                .unwrap_or(false);
            let status = if running { "RUNNING" } else { "STOPPED" };
            let is_target = format!("{}:{}", t.target.host, t.target.port);
            let jumps = t.jumps.len().to_string();
            Row::new(vec![
                (i + 1).to_string(),
                t.name.clone(),
                status.to_string(),
                t.local_port.to_string(),
                is_target,
                jumps,
            ])
            .style(style)
        })
        .collect();

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
        Constraint::Length(9),
        Constraint::Min(5),
        Constraint::Length(6),
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
        " Jump Hosts [a=add, e=edit, x=remove] ",
    );
    let jumps_block = Block::default()
        .title(jumps_title)
        .borders(Borders::ALL);
    let jumps_inner = jumps_block.inner(chunks[1]);
    frame.render_widget(jumps_block, chunks[1]);

    let jumps_chunks = Layout::new(Direction::Vertical, [
        Constraint::Min(3),
        Constraint::Length(3),
    ])
    .split(jumps_inner);

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
            ListItem::new(format!("  {} {}{}:{}", i + 1, user, j.host, j.port)).style(if i == form.selected_jump {
                Style::default()
                    .bg(Color::DarkGray)
                    .fg(Color::White)
            } else {
                Style::default()
            })
        })
        .collect();

    if items.is_empty() {
        let empty = Paragraph::new("No jump hosts yet. Press 'a' to add one.")
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(empty, jumps_chunks[0]);
    } else {
        let list = ratatui::widgets::List::new(items).highlight_style(
            Style::default().bg(Color::Blue).fg(Color::White),
        );
        frame.render_widget(list, jumps_chunks[0]);
    }

    draw_input(
        frame,
        jumps_chunks[1],
        "Key Path (optional)",
        &form.key_path,
        form.input == InputField::KeyPath,
    );

    // Bottom target fields
    let bottom = Layout::new(Direction::Horizontal, [
        Constraint::Percentage(75),
        Constraint::Percentage(25),
    ])
    .split(chunks[2]);

    draw_input(
        frame,
        bottom[0],
        "Target Host",
        &form.target_host,
        form.input == InputField::TargetHost,
    );
    draw_input(
        frame,
        bottom[1],
        "Target Port",
        &form.target_port,
        form.input == InputField::TargetPort,
    );

    let note = Paragraph::new(
        "Tab: next field | Enter: save | Esc: back",
    )
    .style(Style::default().fg(Color::DarkGray))
    .alignment(Alignment::Center);

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

    let _ = note;
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
        Constraint::Length(5),
        Constraint::Length(3),
        Constraint::Min(2),
    ])
    .split(inner);

    draw_input(frame, chunks[0], "User (optional)", &form.user, form.input == JumpField::User);
    draw_input(frame, chunks[1], "Host", &form.host, form.input == JumpField::Host);
    draw_input(frame, chunks[2], "Port (22)", &form.port, form.input == JumpField::Port);

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
