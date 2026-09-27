use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, BorderType, Clear, Widget},
};

use crate::app::state::Palette;

pub(crate) type Rgb = (u8, u8, u8);

/// Braille spinner for the Working status; frame 0 is shown when motion is off.
pub(crate) const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub(crate) fn mix_rgb(base: Rgb, target: Rgb, amount: f32) -> Rgb {
    fn channel(base: u8, target: u8, amount: f32) -> u8 {
        (f32::from(base) + (f32::from(target) - f32::from(base)) * amount).round() as u8
    }
    (
        channel(base.0, target.0, amount),
        channel(base.1, target.1, amount),
        channel(base.2, target.2, amount),
    )
}

pub(crate) fn relative_luminance(color: Rgb) -> f32 {
    fn channel(value: u8) -> f32 {
        let value = f32::from(value) / 255.0;
        if value <= 0.03928 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    }
    0.2126 * channel(color.0) + 0.7152 * channel(color.1) + 0.0722 * channel(color.2)
}

pub(crate) fn color_to_rgb(color: Color) -> Option<Rgb> {
    match color {
        Color::Reset => None,
        Color::Black => Some((0, 0, 0)),
        Color::Red => Some((128, 0, 0)),
        Color::Green => Some((0, 128, 0)),
        Color::Yellow => Some((128, 128, 0)),
        Color::Blue => Some((0, 0, 128)),
        Color::Magenta => Some((128, 0, 128)),
        Color::Cyan => Some((0, 128, 128)),
        Color::Gray => Some((192, 192, 192)),
        Color::DarkGray => Some((128, 128, 128)),
        Color::LightRed => Some((255, 0, 0)),
        Color::LightGreen => Some((0, 255, 0)),
        Color::LightYellow => Some((255, 255, 0)),
        Color::LightBlue => Some((0, 0, 255)),
        Color::LightMagenta => Some((255, 0, 255)),
        Color::LightCyan => Some((0, 255, 255)),
        Color::White => Some((255, 255, 255)),
        Color::Rgb(r, g, b) => Some((r, g, b)),
        Color::Indexed(_) => None,
    }
}

/// Blend `a` toward `b` by `t`. Only true RGB colours blend: named and indexed
/// colours depend on the host terminal's palette, so `a` is returned unchanged.
pub(crate) fn tint(a: Color, b: Color, t: f32) -> Color {
    match (a, b) {
        (Color::Rgb(ar, ag, ab), Color::Rgb(br, bg, bb)) => {
            let (r, g, b) = mix_rgb((ar, ag, ab), (br, bg, bb), t.clamp(0.0, 1.0));
            Color::Rgb(r, g, b)
        }
        _ => a,
    }
}

/// A half-block pill: `▐` and `▌` caps on the rect's edge cells (drawn in the
/// fill colour over whatever was behind them), the text centred inside.
pub(crate) fn pill(buf: &mut Buffer, rect: Rect, text: &str, fill: Color, fg: Color, bold: bool) {
    let area = rect.intersection(buf.area);
    if area.is_empty() {
        return;
    }
    let capped = area == rect && rect.width >= 3;
    let behind = |buf: &Buffer, x: u16| buf.cell((x, rect.y)).map_or(Color::Reset, |c| c.bg);
    let (left_bg, right_bg) = (behind(buf, rect.x), behind(buf, rect.right() - 1));
    let mut style = Style::default().fg(fg).bg(fill);
    if bold {
        style = style.add_modifier(Modifier::BOLD);
    }
    buf.set_style(area, style);
    let pad = u16::from(capped);
    let inner_w = area.width - 2 * pad;
    let text_w = (unicode_width::UnicodeWidthStr::width(text) as u16).min(inner_w);
    buf.set_stringn(
        area.x + pad + (inner_w - text_w) / 2,
        rect.y,
        text,
        usize::from(text_w),
        style,
    );
    if capped {
        for (x, cap, bg) in [(rect.x, "▐", left_bg), (rect.right() - 1, "▌", right_bg)] {
            buf[(x, rect.y)]
                .set_symbol(cap)
                .set_style(Style::reset().fg(fill).bg(bg));
        }
    }
}

/// Rounded floating panel: clears `rect`, fills it with the panel background,
/// draws a rounded border in `border` and an optional ` ◆ title ` on the top
/// edge. Returns the rect inside the border (no extra padding, so callers keep
/// their layout).
pub(crate) fn float_frame(
    buf: &mut Buffer,
    rect: Rect,
    title: Option<&str>,
    border: Color,
    background: Color,
) -> Rect {
    Clear.render(rect, buf);
    let mut block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border))
        .style(Style::default().bg(background));
    if let Some(title) = title {
        block = block.title_top(
            ratatui::text::Line::from(format!(" ◆ {title} "))
                .style(Style::default().fg(border).add_modifier(Modifier::BOLD)),
        );
    }
    let inner = block.inner(rect);
    block.render(rect, buf);
    inner
}

pub(crate) fn panel_contrast_fg(palette: &Palette) -> Color {
    match palette.panel_bg {
        Color::Reset => palette.surface_dim,
        color => color,
    }
}

pub(crate) fn centered_popup_rect(area: Rect, popup_width: u16, popup_height: u16) -> Option<Rect> {
    let popup_width = popup_width.min(area.width.saturating_sub(4));
    let popup_height = popup_height.min(area.height.saturating_sub(2));
    if popup_width < 4 || popup_height < 4 {
        return None;
    }

    Some(Rect::new(
        area.x + area.width.saturating_sub(popup_width) / 2,
        area.y + area.height.saturating_sub(popup_height) / 2,
        popup_width,
        popup_height,
    ))
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ModalStackAreas {
    pub header: Rect,
    pub content: Rect,
    pub footer: Option<Rect>,
    pub actions: Option<Rect>,
}

pub(crate) fn modal_stack_areas(
    inner: Rect,
    header_height: u16,
    footer_height: u16,
    actions_height: u16,
    gap: u16,
) -> ModalStackAreas {
    #[derive(Clone, Copy)]
    enum Slot {
        Header,
        Content,
        Footer,
        Actions,
    }

    let mut constraints = Vec::new();
    let mut slots = Vec::new();
    let mut push = |slot: Slot, constraint: Constraint| {
        if !slots.is_empty() {
            constraints.push(Constraint::Length(gap));
        }
        constraints.push(constraint);
        slots.push(slot);
    };

    push(Slot::Header, Constraint::Length(header_height));
    push(Slot::Content, Constraint::Min(0));
    if footer_height > 0 {
        push(Slot::Footer, Constraint::Length(footer_height));
    }
    if actions_height > 0 {
        push(Slot::Actions, Constraint::Length(actions_height));
    }

    let areas = Layout::vertical(constraints).split(inner);
    let mut result = ModalStackAreas {
        header: Rect::default(),
        content: Rect::default(),
        footer: None,
        actions: None,
    };
    for (slot, area) in slots.into_iter().zip(areas.iter().step_by(2).copied()) {
        match slot {
            Slot::Header => result.header = area,
            Slot::Content => result.content = area,
            Slot::Footer => result.footer = Some(area),
            Slot::Actions => result.actions = Some(area),
        }
    }
    result
}

fn action_button_width(hint: Option<&str>, label: &str) -> u16 {
    match hint {
        Some(hint) => format!(" {hint} {label} ").chars().count() as u16,
        None => format!(" {label} ").chars().count() as u16,
    }
}

pub(crate) fn close_button_rect(area: Rect) -> Rect {
    let width = action_button_width(Some("esc"), "close");
    Rect::new(area.x + area.width.saturating_sub(width), area.y, width, 1)
}

pub(crate) fn continue_button_rect(area: Rect) -> Rect {
    Rect::new(
        area.x,
        area.y,
        action_button_width(Some("↵"), "continue"),
        1,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tint_blends_rgb_and_falls_back_for_terminal_colours() {
        let a = Color::Rgb(0, 0, 0);
        let b = Color::Rgb(200, 100, 50);
        assert_eq!(tint(a, b, 0.5), Color::Rgb(100, 50, 25));
        assert_eq!(tint(a, b, 0.0), a);
        assert_eq!(tint(Color::Reset, b, 0.5), Color::Reset);
        assert_eq!(tint(a, Color::Reset, 0.5), a);
        assert_eq!(tint(a, Color::Indexed(4), 0.5), a);
    }

    #[test]
    fn pill_puts_caps_on_the_rect_edges_over_the_background_behind() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 12, 1));
        buf.set_style(buf.area, Style::default().bg(Color::Rgb(1, 2, 3)));
        let fill = Color::Rgb(9, 9, 9);
        pill(
            &mut buf,
            Rect::new(2, 0, 8, 1),
            "api",
            fill,
            Color::White,
            true,
        );
        assert_eq!(buf[(2, 0)].symbol(), "▐");
        assert_eq!(buf[(9, 0)].symbol(), "▌");
        assert_eq!(buf[(2, 0)].fg, fill);
        assert_eq!(buf[(2, 0)].bg, Color::Rgb(1, 2, 3));
        let inside = (3..9).map(|x| buf[(x, 0)].symbol()).collect::<String>();
        assert_eq!(inside, " api  ");
        assert!((3..9).all(|x| buf[(x, 0)].bg == fill));
        assert_eq!(buf[(1, 0)].symbol(), " ");
        assert_eq!(buf[(10, 0)].bg, Color::Rgb(1, 2, 3));
    }

    #[test]
    fn float_frame_draws_rounded_corners_and_title() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 20, 4));
        let inner = float_frame(
            &mut buf,
            Rect::new(0, 0, 20, 4),
            Some("Close"),
            Color::Rgb(137, 180, 250),
            Color::Rgb(24, 24, 37),
        );
        assert_eq!(inner, Rect::new(1, 1, 18, 2));
        assert_eq!(buf[(0, 0)].symbol(), "╭");
        assert_eq!(buf[(19, 0)].symbol(), "╮");
        assert_eq!(buf[(0, 3)].symbol(), "╰");
        assert_eq!(buf[(19, 3)].symbol(), "╯");
        let top = (0..20).map(|x| buf[(x, 0)].symbol()).collect::<String>();
        assert!(top.contains(" ◆ Close "), "{top}");
        assert_eq!(buf[(5, 1)].bg, Color::Rgb(24, 24, 37));
    }
}
