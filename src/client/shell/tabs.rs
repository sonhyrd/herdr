use super::*;

const TAB_SCROLL_BUTTON_WIDTH: u16 = 3;
const MAX_TAB_WIDTH: u16 = 24;
const MIN_TAB_STRIP_WIDTH: u16 =
    MIN_TAB_WIDTH + NEW_TAB_WIDTH + TAB_SCROLL_BUTTON_WIDTH.saturating_mul(2);

pub(crate) fn render_tab_bar(
    buffer: &mut Buffer,
    area: Rect,
    snapshot: &ClientShellSnapshot,
    config: &ClientShellConfig,
    tab_scroll: &mut usize,
    reveal_focused_tab: &mut bool,
    tab_drag_insert_index: Option<usize>,
    spin: u8,
    hits: &mut ShellHitMap,
) {
    let palette = &config.palette;
    let modern = config.chrome == crate::config::ChromeStyle::Modern;
    buffer.set_style(area, Style::default().bg(palette.panel_bg));
    let tabs = snapshot
        .tabs
        .iter()
        .filter(|tab| Some(tab.workspace_id.as_str()) == snapshot.focused_workspace_id.as_deref())
        .collect::<Vec<_>>();
    let desired_widths = tabs
        .iter()
        .enumerate()
        .map(|(index, tab)| {
            let (prefix, glyph, label) = tab_parts(tab, index + 1, config, 0);
            display_width(&format!("{prefix}{glyph}{label}"))
                .saturating_add(4)
                .clamp(MIN_TAB_WIDTH, MAX_TAB_WIDTH)
        })
        .collect::<Vec<_>>();
    let content = tab_bar_content_area(snapshot, config, area);
    let mouse_chrome = config.mouse_capture;
    let new_tab_width = if mouse_chrome { NEW_TAB_WIDTH } else { 0 };
    let desired_total = desired_widths
        .iter()
        .copied()
        .fold(0_u16, u16::saturating_add)
        .saturating_add(tabs.len().saturating_sub(1).min(u16::MAX as usize) as u16)
        .saturating_add(new_tab_width);
    let overflow =
        desired_total > content.width && (!mouse_chrome || content.width >= MIN_TAB_STRIP_WIDTH);
    let available = if overflow && mouse_chrome {
        content
            .width
            .saturating_sub(NEW_TAB_WIDTH)
            .saturating_sub(TAB_SCROLL_BUTTON_WIDTH.saturating_mul(2))
    } else {
        content.width.saturating_sub(new_tab_width)
    };
    let max_scroll = max_tab_scroll(&desired_widths, available);
    if !overflow {
        *tab_scroll = 0;
    } else if *reveal_focused_tab {
        if let Some(focused) = tabs.iter().position(|tab| tab.focused) {
            *tab_scroll = centered_tab_scroll(focused, &desired_widths, available).min(max_scroll);
        }
    } else {
        *tab_scroll = (*tab_scroll).min(max_scroll);
    }
    *reveal_focused_tab = false;

    let mut x = content.x;
    let tab_right = if overflow && mouse_chrome {
        hits.tab_scroll_left = Rect::new(
            content.x,
            content.y,
            TAB_SCROLL_BUTTON_WIDTH.min(content.width),
            1,
        );
        put_text(
            buffer,
            hits.tab_scroll_left.x,
            content.y,
            hits.tab_scroll_left.width,
            if modern { " ‹ " } else { " < " },
            Style::default()
                .fg(if *tab_scroll > 0 {
                    palette.overlay1
                } else {
                    palette.overlay0
                })
                .bg(palette.surface0),
        );
        x = hits.tab_scroll_left.right();
        content
            .right()
            .saturating_sub(NEW_TAB_WIDTH + TAB_SCROLL_BUTTON_WIDTH)
    } else {
        content.right().saturating_sub(new_tab_width)
    };

    let mut first_visible = None;
    let mut last_visible = None;
    for (index, tab) in tabs.iter().enumerate().skip(*tab_scroll) {
        let (prefix, glyph, label) = tab_parts(tab, index + 1, config, spin);
        let desired = desired_widths[index];
        let remaining = tab_right.saturating_sub(x);
        let width = desired.min(remaining);
        if width == 0 {
            break;
        }
        let rect = Rect::new(x, area.y, width, 1);
        let style = if tab.focused {
            let base = Style::default()
                .fg(panel_contrast_fg(palette))
                .bg(palette.accent);
            if tab.custom_label {
                base.add_modifier(Modifier::BOLD)
            } else {
                base
            }
        } else if tab.custom_label {
            Style::default().fg(palette.overlay1).bg(palette.surface0)
        } else {
            Style::default().fg(palette.overlay0).bg(palette.surface0)
        };
        let (style, label_style, text_right) = if modern {
            let (fill, style) = modern_tab_style(tab, palette);
            crate::ui::pill(
                buffer,
                rect,
                "",
                fill,
                style.fg.unwrap_or(palette.text),
                false,
            );
            let label_style =
                if tab.agent_status == crate::api::schema::AgentStatus::Blocked && !tab.focused {
                    style.fg(palette.red).add_modifier(Modifier::BOLD)
                } else {
                    style
                };
            (style, label_style, rect.right().saturating_sub(1))
        } else {
            buffer.set_style(rect, style);
            (style, style, rect.right())
        };
        let text_width = display_width(&format!("{prefix}{glyph}{label}"));
        let mut cursor = rect.x + width.saturating_sub(text_width) / 2;
        let glyph_style = style.fg(status_color(tab.agent_status, palette));
        for (part, part_style) in [
            (prefix.as_str(), style),
            (glyph, glyph_style),
            (label.as_str(), label_style),
        ] {
            put_text(
                buffer,
                cursor,
                rect.y,
                text_right.saturating_sub(cursor),
                part,
                part_style,
            );
            cursor = cursor.saturating_add(display_width(part));
        }
        hits.tabs.push((rect, tab.tab_id.clone()));
        first_visible.get_or_insert(index);
        last_visible = Some(index);
        x = x.saturating_add(width + 1);
        if width < desired {
            break;
        }
    }

    if overflow && mouse_chrome {
        hits.tab_scroll_right = Rect::new(tab_right, area.y, TAB_SCROLL_BUTTON_WIDTH, 1);
        let can_scroll_right = *tab_scroll < max_scroll;
        put_text(
            buffer,
            hits.tab_scroll_right.x,
            area.y,
            hits.tab_scroll_right.width,
            if modern { " › " } else { " > " },
            Style::default()
                .fg(if can_scroll_right {
                    palette.overlay1
                } else {
                    palette.overlay0
                })
                .bg(palette.surface0),
        );
        hits.new_tab = Rect::new(
            hits.tab_scroll_right.right(),
            area.y,
            content
                .right()
                .saturating_sub(hits.tab_scroll_right.right())
                .min(NEW_TAB_WIDTH),
            1,
        );
    } else if mouse_chrome {
        hits.new_tab = Rect::new(
            x.min(content.right()),
            area.y,
            content.right().saturating_sub(x).min(NEW_TAB_WIDTH),
            1,
        );
    }
    if mouse_chrome {
        put_text(
            buffer,
            hits.new_tab.x,
            area.y,
            hits.new_tab.width,
            " + ",
            Style::default().fg(palette.overlay1).bg(palette.panel_bg),
        );
    }

    if first_visible.is_some_and(|index| index > 0) {
        let ellipsis_x = if hits.tab_scroll_left.width > 0 {
            hits.tab_scroll_left.right()
        } else {
            content.x
        };
        put_text(
            buffer,
            ellipsis_x,
            area.y,
            u16::from(ellipsis_x < content.right()),
            "…",
            Style::default().fg(palette.overlay0),
        );
    }
    if last_visible.is_some_and(|index| index + 1 < tabs.len()) {
        let ellipsis_x = if hits.tab_scroll_right.width > 0 {
            hits.tab_scroll_right.x.saturating_sub(1)
        } else {
            content.right().saturating_sub(1)
        };
        put_text(
            buffer,
            ellipsis_x,
            area.y,
            u16::from(ellipsis_x >= content.x && ellipsis_x < content.right()),
            "…",
            Style::default().fg(palette.overlay0),
        );
    }

    if let Some(insert_index) = tab_drag_insert_index {
        if let Some(indicator_x) = tab_drop_indicator_x(hits, &tabs, insert_index) {
            put_text(
                buffer,
                indicator_x.min(content.right().saturating_sub(1)),
                area.y,
                1,
                "│",
                Style::default().fg(palette.accent),
            );
        }
    }
    render_tab_bar_status(buffer, area, snapshot, config, spin);
}

/// Modern tab fill: the accent for the focused tab, otherwise surface0 tinted
/// by the tab's agent status (or faintly by the accent when it has none).
fn modern_tab_style(tab: &ClientShellTab, palette: &Palette) -> (ratatui::style::Color, Style) {
    use crate::api::schema::AgentStatus;
    if tab.focused {
        return (
            palette.accent,
            Style::default()
                .fg(panel_contrast_fg(palette))
                .bg(palette.accent)
                .add_modifier(Modifier::BOLD),
        );
    }
    let fill = match tab.agent_status {
        AgentStatus::Idle | AgentStatus::Unknown => {
            crate::ui::tint(palette.surface0, palette.accent, 0.12)
        }
        status => crate::ui::tint(palette.surface0, status_color(status, palette), 0.22),
    };
    (fill, Style::default().fg(palette.overlay1).bg(fill))
}

/// `✕2 ⠹3 ✓1`: how many spaces are blocked, working and done. Zeros are hidden.
fn fleet_counts(
    snapshot: &ClientShellSnapshot,
    config: &ClientShellConfig,
    spin: u8,
) -> Vec<(String, crate::api::schema::AgentStatus)> {
    use crate::api::schema::AgentStatus;
    [
        AgentStatus::Blocked,
        AgentStatus::Working,
        AgentStatus::Done,
    ]
    .into_iter()
    .filter_map(|status| {
        let count = snapshot
            .workspaces
            .iter()
            .filter(|workspace| workspace.agent_status == status)
            .count();
        (count > 0).then(|| {
            let glyph = status_icon_frame(
                status,
                crate::config::StatusIndicatorStyle::Symbols,
                config.chrome,
                spin,
            );
            (format!("{glyph}{count}"), status)
        })
    })
    .collect()
}

fn fleet_width(fleet: &[(String, crate::api::schema::AgentStatus)]) -> u16 {
    fleet
        .iter()
        .map(|(text, _)| display_width(text))
        .fold(0u16, u16::saturating_add)
        .saturating_add(fleet.len().saturating_sub(1) as u16)
}

pub(crate) fn tab_bar_status_width(
    snapshot: &ClientShellSnapshot,
    config: &ClientShellConfig,
) -> u16 {
    let modern = config.chrome == crate::config::ChromeStyle::Modern;
    let pill_caps = if modern { 2 } else { 0 };
    let content = snapshot.tab_bar_right.iter().fold(0u16, |width, segment| {
        width.saturating_add(display_width(&segment.text) + pill_caps)
    });
    let separators = snapshot.tab_bar_right.len().saturating_sub(1);
    let user = content.saturating_add(
        display_width(&snapshot.tab_bar_right_separator)
            .saturating_mul(separators.min(u16::MAX as usize) as u16),
    );
    if !modern {
        return user;
    }
    let fleet = fleet_width(&fleet_counts(snapshot, config, 0));
    match (fleet, user) {
        (0, user) => user,
        (fleet, 0) => fleet,
        (fleet, user) => fleet.saturating_add(2).saturating_add(user),
    }
}

fn tab_bar_status_area(
    snapshot: &ClientShellSnapshot,
    config: &ClientShellConfig,
    area: Rect,
) -> Option<Rect> {
    let width = tab_bar_status_width(snapshot, config);
    if width == 0 {
        return None;
    }
    let reserved = width.saturating_add(1);
    (area.width.saturating_sub(reserved) >= MIN_TAB_STRIP_WIDTH)
        .then(|| Rect::new(area.right().saturating_sub(width), area.y, width, 1))
}

fn tab_bar_content_area(
    snapshot: &ClientShellSnapshot,
    config: &ClientShellConfig,
    area: Rect,
) -> Rect {
    let reserved = tab_bar_status_area(snapshot, config, area)
        .map(|status| status.width.saturating_add(1))
        .unwrap_or(0);
    Rect {
        width: area.width.saturating_sub(reserved),
        ..area
    }
}

fn render_tab_bar_status(
    buffer: &mut Buffer,
    area: Rect,
    snapshot: &ClientShellSnapshot,
    config: &ClientShellConfig,
    spin: u8,
) {
    let palette = &config.palette;
    let Some(status) = tab_bar_status_area(snapshot, config, area) else {
        return;
    };
    let modern = config.chrome == crate::config::ChromeStyle::Modern;
    let separator_width = display_width(&snapshot.tab_bar_right_separator);
    let mut x = status.x;
    if modern {
        let fleet = fleet_counts(snapshot, config, spin);
        for (index, (text, status)) in fleet.iter().enumerate() {
            if index > 0 {
                x = x.saturating_add(1);
            }
            let width = display_width(text);
            put_text(
                buffer,
                x,
                area.y,
                width,
                text,
                Style::default()
                    .fg(status_color(*status, palette))
                    .bg(palette.panel_bg)
                    .add_modifier(Modifier::BOLD),
            );
            x = x.saturating_add(width);
        }
        if !fleet.is_empty() && !snapshot.tab_bar_right.is_empty() {
            x = x.saturating_add(2);
        }
    }
    for (index, segment) in snapshot.tab_bar_right.iter().enumerate() {
        if index > 0 && separator_width > 0 {
            put_text(
                buffer,
                x,
                area.y,
                separator_width,
                &snapshot.tab_bar_right_separator,
                Style::default().fg(palette.overlay0).bg(palette.panel_bg),
            );
            x = x.saturating_add(separator_width);
        }
        let width = display_width(&segment.text);
        if modern {
            let (fill, fg) = if segment.accent {
                (palette.accent, panel_contrast_fg(palette))
            } else {
                (palette.surface0, palette.overlay1)
            };
            let rect = Rect::new(x, area.y, width + 2, 1);
            crate::ui::pill(buffer, rect, &segment.text, fill, fg, segment.accent);
            x = x.saturating_add(width + 2);
            continue;
        }
        let style = if segment.accent {
            Style::default()
                .fg(panel_contrast_fg(palette))
                .bg(palette.accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(palette.overlay1).bg(palette.panel_bg)
        };
        put_text(buffer, x, area.y, width, &segment.text, style);
        x = x.saturating_add(width);
    }
}

fn tab_drop_indicator_x(
    hits: &ShellHitMap,
    tabs: &[&ClientShellTab],
    insert_index: usize,
) -> Option<u16> {
    let visible = hits
        .tabs
        .iter()
        .filter_map(|(rect, tab_id)| {
            tabs.iter()
                .position(|tab| tab.tab_id == *tab_id)
                .map(|index| (index, *rect))
        })
        .collect::<Vec<_>>();
    let (first_index, first_rect) = *visible.first()?;
    let (last_index, last_rect) = *visible.last()?;
    if insert_index == 0 {
        return Some(if first_index == 0 {
            first_rect.x
        } else {
            hits.tab_scroll_left.right()
        });
    }
    if let Some((_, rect)) = visible.iter().find(|(index, _)| *index == insert_index) {
        return Some(rect.x.saturating_sub(1));
    }
    if insert_index >= tabs.len() {
        return Some(if last_index + 1 >= tabs.len() {
            last_rect.right()
        } else {
            hits.tab_scroll_right.x.saturating_sub(1)
        });
    }
    None
}

fn centered_tab_scroll(focused: usize, widths: &[u16], available: u16) -> usize {
    let mut best = focused;
    let mut best_distance = u16::MAX;
    for start in 0..=focused {
        let before = widths
            .iter()
            .copied()
            .enumerate()
            .skip(start)
            .take(focused.saturating_sub(start))
            .fold(0u16, |width, (_, tab)| width.saturating_add(tab + 1));
        if before >= available {
            continue;
        }
        let focused_width = widths[focused].min(available.saturating_sub(before));
        let center = before.saturating_mul(2).saturating_add(focused_width);
        let distance = center.abs_diff(available);
        if distance <= best_distance {
            best_distance = distance;
            best = start;
        }
    }
    best
}

fn max_tab_scroll(widths: &[u16], available: u16) -> usize {
    let Some((&last, preceding)) = widths.split_last() else {
        return 0;
    };
    let mut start = preceding.len();
    let mut used = u32::from(last);
    // Keep the longest fully visible suffix, not merely a sliver of the last tab.
    // An oversized last tab must still be reachable at the start of the strip.
    for width in preceding.iter().rev() {
        let required = used + 1 + u32::from(*width);
        if required > u32::from(available) {
            break;
        }
        used = required;
        start -= 1;
    }
    start
}

/// `1 ◐ api-server`: position (what switch-tab and the auto label use, not the
/// stable public `number`), status glyph, then the custom label, cut so the tab
/// fits `MAX_TAB_WIDTH`. A label equal to the position (the auto name) is skipped.
fn tab_parts(
    tab: &ClientShellTab,
    position: usize,
    config: &ClientShellConfig,
    spin: u8,
) -> (String, &'static str, String) {
    let prefix = format!("{position} ");
    let glyph = status_icon_frame(
        tab.agent_status,
        config.status_indicators,
        config.chrome,
        spin,
    );
    let mut label = if tab.custom_label || tab.label != position.to_string() {
        format!(" {}", tab.label)
    } else {
        String::new()
    };
    if tab.zoomed {
        label.push_str(" Z");
    }
    let budget = MAX_TAB_WIDTH.saturating_sub(4) as usize;
    let used = (display_width(&prefix) + display_width(glyph)) as usize;
    let label = crate::ui::truncate_end(&label, budget.saturating_sub(used));
    (prefix, glyph, label)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::schema::AgentStatus;

    fn render(
        snapshot: &ClientShellSnapshot,
        chrome: crate::config::ChromeStyle,
    ) -> (Buffer, ShellHitMap) {
        let mut config = ClientShellConfig::from_config(&Config::default());
        config.status_indicators = crate::config::StatusIndicatorStyle::Symbols;
        config.chrome = chrome;
        let area = Rect::new(0, 0, 80, 1);
        let mut buffer = Buffer::empty(area);
        let mut hits = ShellHitMap::default();
        render_tab_bar(
            &mut buffer,
            area,
            snapshot,
            &config,
            &mut 0,
            &mut false,
            None,
            0,
            &mut hits,
        );
        (buffer, hits)
    }

    fn row(buffer: &Buffer) -> String {
        buffer.content.iter().map(|cell| cell.symbol()).collect()
    }

    #[test]
    fn modern_tabs_keep_classic_geometry_and_cap_their_padding() {
        let mut snapshot = crate::client::shell::tests::snapshot();
        let mut second = snapshot.tabs[0].clone();
        second.tab_id = "tab_2".into();
        second.label = "web".into();
        second.custom_label = true;
        second.focused = false;
        second.agent_status = AgentStatus::Blocked;
        snapshot.tabs.push(second);

        let (_, classic) = render(&snapshot, crate::config::ChromeStyle::Classic);
        let (buffer, modern) = render(&snapshot, crate::config::ChromeStyle::Modern);
        assert_eq!(classic.tabs, modern.tabs);
        assert_eq!(classic.new_tab, modern.new_tab);
        let (rect, _) = modern.tabs[1];
        assert_eq!(buffer[(rect.x, 0)].symbol(), "▐");
        assert_eq!(buffer[(rect.right() - 1, 0)].symbol(), "▌");
        assert!(row(&buffer).contains("2 ✕ web"), "{}", row(&buffer));
    }

    #[test]
    fn fleet_counts_hide_zero_statuses() {
        let mut snapshot = crate::client::shell::tests::snapshot();
        let mut working = snapshot.workspaces[0].clone();
        working.workspace_id = "ws_2".into();
        working.focused = false;
        working.agent_status = AgentStatus::Working;
        let mut done = working.clone();
        done.workspace_id = "ws_3".into();
        done.agent_status = AgentStatus::Done;
        snapshot.workspaces.extend([working.clone(), working, done]);

        let (buffer, _) = render(&snapshot, crate::config::ChromeStyle::Modern);
        let text = row(&buffer);
        assert!(text.trim_end().ends_with("◐2 ✓1"), "{text}");
        assert!(!text.contains('✕'), "{text}");

        let (classic, _) = render(&snapshot, crate::config::ChromeStyle::Classic);
        assert!(!row(&classic).contains("✓1"));
    }

    #[test]
    fn trailing_scroll_limit_accounts_for_full_widths_and_separators() {
        for (widths, available, expected) in [
            (&[][..], 0, 0),
            (&[8, 13][..], 0, 1),
            (&[8, 13][..], 1, 1),
            (&[8, 13][..], 12, 1),
            (&[8, 13][..], 21, 1),
            (&[8, 13][..], 22, 0),
            (&[8, 13][..], 30, 0),
            (&[8, u16::MAX][..], u16::MAX, 1),
        ] {
            assert_eq!(
                max_tab_scroll(widths, available),
                expected,
                "widths={widths:?}, available={available}"
            );
        }
    }
}
