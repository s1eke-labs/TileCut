use std::path::PathBuf;

use crate::cli::{LayoutMode, OutputFormat};

pub fn zero_pad_width(cols: u32, rows: u32) -> usize {
    let max_index = cols.saturating_sub(1).max(rows.saturating_sub(1));
    usize::max(4, digit_count(max_index))
}

fn digit_count(value: u32) -> usize {
    value.checked_ilog10().unwrap_or(0) as usize + 1
}

pub fn render_rel_path(
    layout: LayoutMode,
    format: OutputFormat,
    pad_width: usize,
    x: u32,
    y: u32,
) -> PathBuf {
    let x = format!("{x:0pad_width$}");
    let y = format!("{y:0pad_width$}");
    match layout {
        LayoutMode::Flat => PathBuf::from(format!("tiles/x{x}_y{y}.{}", format.extension())),
        LayoutMode::Sharded => PathBuf::from(format!("tiles/y{y}/x{x}.{}", format.extension())),
    }
}

pub fn path_template(layout: LayoutMode, format: OutputFormat) -> String {
    match layout {
        LayoutMode::Flat => format!("tiles/x{{x}}_y{{y}}.{}", format.extension()),
        LayoutMode::Sharded => format!("tiles/y{{y}}/x{{x}}.{}", format.extension()),
    }
}

#[cfg(test)]
mod tests {
    use super::{path_template, render_rel_path, zero_pad_width};
    use crate::cli::{LayoutMode, OutputFormat};

    #[test]
    fn renders_flat_paths_with_zero_padding() {
        let path = render_rel_path(LayoutMode::Flat, OutputFormat::Png, 4, 3, 12);
        assert_eq!(path.to_string_lossy(), "tiles/x0003_y0012.png");
    }

    #[test]
    fn renders_sharded_template() {
        assert_eq!(
            path_template(LayoutMode::Sharded, OutputFormat::Webp),
            "tiles/y{y}/x{x}.webp"
        );
    }

    #[test]
    fn zero_padding_has_minimum_width() {
        assert_eq!(zero_pad_width(3, 9), 4);
    }
}
