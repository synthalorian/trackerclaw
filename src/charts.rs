use std::fmt::Write;

const COLORS: &[&str] = &[
    "#00ffff", "#ff00ff", "#ffff00", "#00ff00", "#ff6600", "#ff0066", "#6600ff", "#00ff66",
    "#ff3333", "#33ff33",
];

pub fn bar_chart(data: &[(String, f64)], title: &str, width: u32, height: u32) -> String {
    if data.is_empty() {
        return format!(
            r##"<svg width="{}" height="{}" xmlns="http://www.w3.org/2000/svg">
                <text x="50%" y="50%" text-anchor="middle" fill="#888" font-family="monospace">No data</text>
            </svg>"##,
            width, height
        );
    }

    let margin_top = 40;
    let margin_bottom = 60;
    let margin_left = 50;
    let margin_right = 20;
    let chart_w = width - margin_left - margin_right;
    let chart_h = height - margin_top - margin_bottom;

    let max_val = data
        .iter()
        .map(|(_, v)| *v)
        .fold(0.0_f64, f64::max)
        .max(1.0);
    let bar_width = chart_w as f64 / data.len() as f64 * 0.7;
    let bar_gap = chart_w as f64 / data.len() as f64 * 0.3;

    let mut svg = String::new();
    let _ = write!(
        svg,
        r##"<svg width="{}" height="{}" xmlns="http://www.w3.org/2000/svg" style="background:#0a0a12">
            <text x="{}" y="25" text-anchor="middle" fill="#00ffff" font-size="14" font-family="monospace" font-weight="bold">{}</text>"##,
        width,
        height,
        width / 2,
        title
    );

    // Y-axis labels and grid lines
    for i in 0..=4 {
        let val = max_val * i as f64 / 4.0;
        let y = margin_top as f64 + chart_h as f64 - (i as f64 / 4.0) * chart_h as f64;
        let _ = write!(
            svg,
            r##"<text x="{}" y="{}" text-anchor="end" fill="#888" font-size="10" font-family="monospace">{:.0}h</text>
            <line x1="{}" y1="{}" x2="{}" y2="{}" stroke="#333" stroke-width="1"/>"##,
            margin_left - 5,
            y + 4.0,
            val,
            margin_left,
            y,
            margin_left + chart_w,
            y
        );
    }

    // Bars and X labels
    for (i, (label, value)) in data.iter().enumerate() {
        let x = margin_left as f64 + i as f64 * (bar_width + bar_gap) + bar_gap / 2.0;
        let bar_h = (value / max_val) * chart_h as f64;
        let y = margin_top as f64 + chart_h as f64 - bar_h;
        let color = if i % 2 == 0 { "#00ffff" } else { "#ff00ff" };

        let _ = write!(
            svg,
            r##"<rect x="{}" y="{}" width="{}" height="{}" fill="{}" rx="2" opacity="0.8"/>
            <text x="{}" y="{}" text-anchor="middle" fill="#e0e0e0" font-size="10" font-family="monospace">{:.1}h</text>"##,
            x,
            y,
            bar_width,
            bar_h,
            color,
            x + bar_width / 2.0,
            y - 5.0,
            value
        );

        let show_label = data.len() <= 14 || i % 2 == 0;
        if show_label {
            let label_text = if label.len() > 6 { &label[..6] } else { label };
            let _ = write!(
                svg,
                r##"<text x="{}" y="{}" text-anchor="end" fill="#888" font-size="9" font-family="monospace" transform="rotate(-45 {}, {})">{}</text>"##,
                x + bar_width / 2.0,
                height - margin_bottom + 15,
                x + bar_width / 2.0,
                height - margin_bottom + 15,
                label_text
            );
        }
    }

    // Axes lines
    let _ = write!(
        svg,
        r##"<line x1="{}" y1="{}" x2="{}" y2="{}" stroke="#333" stroke-width="2"/>
        <line x1="{}" y1="{}" x2="{}" y2="{}" stroke="#333" stroke-width="2"/>
        </svg>"##,
        margin_left,
        margin_top + chart_h,
        margin_left + chart_w,
        margin_top + chart_h,
        margin_left,
        margin_top,
        margin_left,
        margin_top + chart_h
    );

    svg
}

pub fn pie_chart(data: &[(String, i64)], title: &str, width: u32, height: u32) -> String {
    if data.is_empty() {
        return format!(
            r##"<svg width="{}" height="{}" xmlns="http://www.w3.org/2000/svg">
                <text x="50%" y="50%" text-anchor="middle" fill="#888" font-family="monospace">No data</text>
            </svg>"##,
            width, height
        );
    }

    let total: i64 = data.iter().map(|(_, v)| *v).sum();
    if total == 0 {
        return format!(
            r##"<svg width="{}" height="{}" xmlns="http://www.w3.org/2000/svg">
                <text x="50%" y="50%" text-anchor="middle" fill="#888" font-family="monospace">No data</text>
            </svg>"##,
            width, height
        );
    }

    let cx = width as f64 / 2.0 - 60.0;
    let cy = height as f64 / 2.0 + 10.0;
    let radius = (height as f64 * 0.35).min(width as f64 * 0.3);

    let mut svg = String::new();
    let _ = write!(
        svg,
        r##"<svg width="{}" height="{}" xmlns="http://www.w3.org/2000/svg" style="background:#0a0a12">
            <text x="{}" y="25" text-anchor="middle" fill="#00ffff" font-size="14" font-family="monospace" font-weight="bold">{}</text>
            <text x="{}" y="{}" text-anchor="middle" fill="#888" font-size="11" font-family="monospace">Total: {:.1}h</text>"##,
        width,
        height,
        width / 2,
        title,
        cx,
        cy + radius + 20.0,
        total as f64 / 3600.0
    );

    let mut start_angle = 0.0_f64;

    for (i, (label, value)) in data.iter().enumerate() {
        let fraction = *value as f64 / total as f64;
        let angle = fraction * 360.0;
        let end_angle = start_angle + angle;
        let color = COLORS[i % COLORS.len()];

        if angle > 0.1 {
            let x1 = cx + radius * (start_angle * std::f64::consts::PI / 180.0).cos();
            let y1 = cy + radius * (start_angle * std::f64::consts::PI / 180.0).sin();
            let x2 = cx + radius * (end_angle * std::f64::consts::PI / 180.0).cos();
            let y2 = cy + radius * (end_angle * std::f64::consts::PI / 180.0).sin();
            let large_arc = if angle > 180.0 { 1 } else { 0 };

            let _ = write!(
                svg,
                r##"<path d="M {},{} L {},{} A {},{} 0 {},1 {},{} Z" fill="{}" opacity="0.85" stroke="#0a0a12" stroke-width="2"/>
                <text x="{}" y="{}" text-anchor="middle" fill="#e0e0e0" font-size="11" font-family="monospace" font-weight="bold">{:.1}%</text>"##,
                cx,
                cy,
                x1,
                y1,
                radius,
                radius,
                large_arc,
                x2,
                y2,
                color,
                cx + (radius * 0.7)
                    * ((start_angle + angle / 2.0) * std::f64::consts::PI / 180.0).cos(),
                cy + (radius * 0.7)
                    * ((start_angle + angle / 2.0) * std::f64::consts::PI / 180.0).sin()
                    + 4.0,
                fraction * 100.0
            );
        }

        // Legend
        let hours = *value as f64 / 3600.0;
        let ly = 50 + i as u32 * 22;
        let display_label = if label.len() > 18 {
            format!("{}...", &label[..15])
        } else {
            label.clone()
        };
        let _ = write!(
            svg,
            r##"<rect x="{}" y="{}" width="12" height="12" fill="{}" rx="2"/>
            <text x="{}" y="{}" fill="#e0e0e0" font-size="11" font-family="monospace">{} ({:.1}h)</text>"##,
            width - 140,
            ly,
            color,
            width - 125,
            ly + 10,
            display_label,
            hours
        );

        start_angle = end_angle;
    }

    svg.push_str("</svg>");
    svg
}

pub fn format_hours(seconds: i64) -> f64 {
    seconds as f64 / 3600.0
}

const BLOCKS: &[char] = &['█', '▉', '▊', '▋', '▌', '▍', '▎', '▏'];

pub fn tui_bar_chart(
    data: &[(String, f64)],
    title: &str,
    max_width: usize,
    max_height: usize,
) -> Vec<String> {
    if data.is_empty() {
        return vec![title.to_string(), "No data".to_string()];
    }

    let mut lines = Vec::new();
    lines.push(title.to_string());

    let max_val = data
        .iter()
        .map(|(_, v)| *v)
        .fold(0.0_f64, f64::max)
        .max(1.0);
    let label_width = data.iter().map(|(l, _)| l.len()).max().unwrap_or(0).min(12);
    let bar_max_width = max_width.saturating_sub(label_width + 4);
    let chart_height = max_height.saturating_sub(2).max(1);

    for (label, value) in data.iter().take(chart_height) {
        let ratio = value / max_val;
        let bar_len = (ratio * bar_max_width as f64) as usize;
        let partial = ((ratio * bar_max_width as f64).fract() * BLOCKS.len() as f64) as usize;

        let mut bar = String::new();
        for _ in 0..bar_len {
            bar.push(BLOCKS[0]);
        }
        if partial > 0 && bar_len < bar_max_width {
            bar.push(BLOCKS[BLOCKS.len() - partial]);
        }

        let label_trunc = if label.len() > label_width {
            format!("{:.width$}", label, width = label_width)
        } else {
            format!("{:>width$}", label, width = label_width)
        };

        let value_label = format!("{:.1}h", value);
        // bar.len() counts bytes; block chars are multi-byte, so count chars.
        // The field width must absorb the unused bar space plus the label
        // itself so all value labels right-align to the same column.
        let pad =
            bar_max_width.saturating_sub(bar.chars().count()) + 1 + value_label.chars().count();
        lines.push(format!("{} │{}{:>pad$}", label_trunc, bar, value_label));
    }

    let axis = format!("{:>width$} └", "", width = label_width);
    let axis_line: String = std::iter::repeat_n('─', bar_max_width.min(max_width)).collect();
    lines.push(format!("{}{}", axis, axis_line));

    lines
}

pub fn tui_project_chart(data: &[(String, i64)], title: &str, max_width: usize) -> Vec<String> {
    let converted: Vec<(String, f64)> = data
        .iter()
        .map(|(label, seconds)| (label.clone(), format_hours(*seconds)))
        .collect();
    tui_bar_chart(&converted, title, max_width, data.len().max(5))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bar_chart_renders_svg() {
        let data = vec![("06-01".to_string(), 2.0), ("06-02".to_string(), 4.5)];
        let svg = bar_chart(&data, "Test", 700, 300);
        assert!(svg.contains("<svg"));
        assert!(svg.contains("</svg>"));
        assert!(svg.contains("<rect"));
    }

    #[test]
    fn bar_chart_empty_data() {
        let svg = bar_chart(&[], "Empty", 700, 300);
        assert!(svg.contains("No data"));
    }

    #[test]
    fn pie_chart_renders_and_empty_safe() {
        let data = vec![("rust".to_string(), 3600), ("docs".to_string(), 1800)];
        let svg = pie_chart(&data, "Breakdown", 700, 350);
        assert!(svg.contains("<svg"));
        assert!(svg.contains("66.7%") || svg.contains("66.6%"));
        assert!(svg.contains("rust"));

        assert!(pie_chart(&[], "x", 700, 350).contains("No data"));
        assert!(pie_chart(&[("zero".to_string(), 0)], "x", 700, 350).contains("No data"));
    }

    #[test]
    fn pie_chart_single_slice_full_circle() {
        // A single 100% slice crosses the 180° large-arc threshold.
        let svg = pie_chart(&[("all".to_string(), 3600)], "One", 700, 350);
        assert!(svg.contains("100.0%"));
    }

    #[test]
    fn tui_bar_chart_alignment_counts_chars_not_bytes() {
        let data = vec![("a".to_string(), 1.0), ("b".to_string(), 2.0)];
        let lines = tui_bar_chart(&data, "T", 40, 10);
        // All bar rows should have the same display width.
        let widths: Vec<usize> = lines[1..3].iter().map(|l| l.chars().count()).collect();
        assert_eq!(widths[0], widths[1], "misaligned: {:?}", lines);
    }

    #[test]
    fn tui_bar_chart_empty() {
        let lines = tui_bar_chart(&[], "T", 40, 10);
        assert_eq!(lines.len(), 2);
        assert!(lines[1].contains("No data"));
    }
}
