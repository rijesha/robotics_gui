//! Designs: complete looks for the station. Each pairs a colour palette and shapes with
//! the layout it was drawn for, so choosing one changes how the whole application reads.

use crate::panels::DEFAULT_LAYOUT;
use crate::theme::{self, Palette};
use crate::workspace::{DesktopLayout, Layout};
use egui::Color32;
use std::sync::atomic::{AtomicUsize, Ordering};

pub struct Design {
    pub name: &'static str,
    /// Name on the command line and in the address bar.
    pub key: &'static str,
    pub about: &'static str,
    pub dark: Palette,
    pub light: Palette,
    /// The theme the design is meant to be seen in first.
    pub starts_dark: bool,
    /// Corner radius of buttons and inputs, and of cards.
    pub radius: u8,
    pub card_radius: u8,
    /// Outline width of cards.
    pub outline: f32,
    /// Everything is drawn this much larger.
    pub zoom: f32,
    /// Opacity of what floats over the map, 0 to 255.
    pub glass: u8,
    /// Whether the big-number telemetry strip sits along the bottom of the map.
    pub strip: bool,
    pub layout: Layout,
    pub desktop: DesktopLayout,
}

const fn rgb(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

/// Two columns with nothing to the left of the map: fewer, larger panes.
const TWO_COLUMNS: DesktopLayout = DesktopLayout {
    left_top: &[],
    left_bottom: &[],
    center: &["Map"],
    right_top: &["Drive", "Video", "Lidar"],
    right_bottom: &["Motors", "Power", "Mission", "Settings", "RTK Base", "Link"],
    right_top_share: 0.47,
};

pub const DESIGNS: [Design; 5] = [
    Design {
        name: "Classic",
        key: "classic",
        about: "Docked panels around the map",
        dark: theme::DARK,
        light: theme::LIGHT,
        starts_dark: true,
        radius: 6,
        card_radius: 8,
        outline: 1.0,
        zoom: 1.0,
        glass: 236,
        strip: false,
        layout: Layout::Dock,
        desktop: DEFAULT_LAYOUT,
    },
    Design {
        name: "Horizon",
        key: "horizon",
        about: "The map edge to edge under glass cards, with the numbers that matter in one strip",
        dark: Palette {
            bg: rgb(8, 13, 22),
            surface: rgb(14, 22, 36),
            raised: rgb(27, 39, 60),
            border: rgb(42, 58, 86),
            text: rgb(228, 238, 250),
            dim: rgb(132, 152, 182),
            accent: rgb(34, 211, 238),
            on_accent: rgb(4, 22, 30),
            ok: rgb(52, 211, 153),
            warn: rgb(251, 191, 36),
            bad: rgb(248, 113, 113),
            viewport: rgb(5, 9, 16),
            viewport_grid: rgb(36, 50, 74),
        },
        light: Palette {
            bg: rgb(236, 242, 248),
            surface: rgb(255, 255, 255),
            raised: rgb(224, 233, 243),
            border: rgb(198, 211, 226),
            text: rgb(15, 23, 42),
            dim: rgb(86, 102, 124),
            accent: rgb(8, 145, 178),
            on_accent: Color32::WHITE,
            ok: rgb(5, 150, 105),
            warn: rgb(180, 110, 0),
            bad: rgb(220, 38, 38),
            viewport: rgb(222, 230, 240),
            viewport_grid: rgb(186, 198, 214),
        },
        starts_dark: true,
        radius: 9,
        card_radius: 14,
        outline: 1.0,
        zoom: 1.0,
        glass: 205,
        strip: true,
        layout: Layout::Map,
        desktop: DEFAULT_LAYOUT,
    },
    Design {
        name: "Workbench",
        key: "workbench",
        about: "A rail of tools beside the map; one tool open at a time, at full height",
        dark: Palette {
            bg: rgb(24, 23, 22),
            surface: rgb(34, 33, 31),
            raised: rgb(50, 48, 45),
            border: rgb(66, 63, 59),
            text: rgb(238, 235, 230),
            dim: rgb(160, 154, 145),
            accent: rgb(139, 148, 250),
            on_accent: rgb(18, 18, 44),
            ok: rgb(74, 190, 120),
            warn: rgb(234, 170, 60),
            bad: rgb(240, 96, 90),
            viewport: rgb(16, 15, 14),
            viewport_grid: rgb(58, 55, 52),
        },
        light: Palette {
            bg: rgb(245, 244, 241),
            surface: rgb(255, 255, 255),
            raised: rgb(236, 234, 229),
            border: rgb(218, 215, 208),
            text: rgb(32, 30, 27),
            dim: rgb(112, 108, 100),
            accent: rgb(79, 70, 229),
            on_accent: Color32::WHITE,
            ok: rgb(21, 128, 61),
            warn: rgb(180, 83, 9),
            bad: rgb(200, 40, 40),
            viewport: rgb(232, 230, 225),
            viewport_grid: rgb(196, 192, 184),
        },
        starts_dark: false,
        radius: 6,
        card_radius: 10,
        outline: 1.0,
        zoom: 1.0,
        glass: 244,
        strip: true,
        layout: Layout::Focus,
        desktop: DEFAULT_LAYOUT,
    },
    Design {
        name: "Fleet board",
        key: "fleet",
        about: "Every vehicle on a card beside the map, and the selected one in detail",
        dark: Palette {
            bg: rgb(17, 17, 19),
            surface: rgb(27, 27, 30),
            raised: rgb(43, 43, 48),
            border: rgb(60, 60, 66),
            text: rgb(242, 242, 244),
            dim: rgb(152, 152, 160),
            accent: rgb(163, 230, 53),
            on_accent: rgb(20, 30, 5),
            ok: rgb(74, 222, 128),
            warn: rgb(250, 204, 21),
            bad: rgb(244, 88, 88),
            viewport: rgb(11, 11, 13),
            viewport_grid: rgb(52, 52, 58),
        },
        light: Palette {
            bg: rgb(243, 243, 245),
            surface: rgb(255, 255, 255),
            raised: rgb(231, 231, 235),
            border: rgb(208, 208, 214),
            text: rgb(24, 24, 27),
            dim: rgb(100, 100, 110),
            accent: rgb(77, 124, 15),
            on_accent: Color32::WHITE,
            ok: rgb(22, 130, 60),
            warn: rgb(170, 100, 0),
            bad: rgb(200, 40, 40),
            viewport: rgb(228, 228, 232),
            viewport_grid: rgb(190, 190, 198),
        },
        starts_dark: true,
        radius: 5,
        card_radius: 6,
        outline: 1.0,
        zoom: 1.0,
        glass: 240,
        strip: false,
        layout: Layout::Fleet,
        desktop: DEFAULT_LAYOUT,
    },
    Design {
        name: "Daylight",
        key: "daylight",
        about: "High contrast and large type for a tablet in the sun: two columns, big targets",
        dark: Palette {
            bg: rgb(0, 0, 0),
            surface: rgb(14, 14, 16),
            raised: rgb(40, 42, 46),
            border: rgb(112, 118, 128),
            text: rgb(255, 255, 255),
            dim: rgb(192, 196, 202),
            accent: rgb(96, 172, 255),
            on_accent: rgb(0, 0, 0),
            ok: rgb(60, 220, 120),
            warn: rgb(255, 190, 40),
            bad: rgb(255, 92, 92),
            viewport: rgb(0, 0, 0),
            viewport_grid: rgb(62, 66, 72),
        },
        light: Palette {
            bg: rgb(255, 255, 255),
            surface: rgb(255, 255, 255),
            raised: rgb(228, 232, 238),
            border: rgb(96, 104, 118),
            text: rgb(8, 10, 14),
            dim: rgb(58, 66, 80),
            accent: rgb(0, 82, 204),
            on_accent: Color32::WHITE,
            ok: rgb(0, 118, 50),
            warn: rgb(166, 84, 0),
            bad: rgb(200, 0, 24),
            viewport: rgb(238, 240, 244),
            viewport_grid: rgb(168, 174, 186),
        },
        starts_dark: false,
        radius: 4,
        card_radius: 4,
        outline: 1.5,
        zoom: 1.2,
        glass: 255,
        strip: false,
        layout: Layout::Dock,
        desktop: TWO_COLUMNS,
    },
];

static CURRENT: AtomicUsize = AtomicUsize::new(0);

/// The design in use.
pub fn current() -> &'static Design {
    &DESIGNS[CURRENT.load(Ordering::Relaxed)]
}

pub fn current_index() -> usize {
    CURRENT.load(Ordering::Relaxed)
}

/// Position of the design with this command-line name.
pub fn find(key: &str) -> Option<usize> {
    DESIGNS.iter().position(|d| d.key.eq_ignore_ascii_case(key))
}

/// Switch design: colours, shapes and size. The caller arranges the workspace to match.
pub fn apply(ctx: &egui::Context, index: usize, dark: bool) {
    CURRENT.store(index.min(DESIGNS.len() - 1), Ordering::Relaxed);
    theme::install(ctx, dark);
    ctx.set_zoom_factor(current().zoom);
}
