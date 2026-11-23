slint::include_modules!();

mod config;

use std::collections::HashMap;
use std::ffi::CString;
use std::fs;
use std::path::{Path, PathBuf};

use slint::{Color, ComponentHandle, Model, ModelRc, SharedString, VecModel};

use crate::config::{Config, CustomColors, ThemeChoice};

impl From<ThemeChoice> for ThemePreference {
    fn from(choice: ThemeChoice) -> Self {
        match choice {
            ThemeChoice::System => Self::System,
            ThemeChoice::Light => Self::Light,
            ThemeChoice::Dark => Self::Dark,
            ThemeChoice::Latte => Self::Latte,
            ThemeChoice::Frappe => Self::Frappe,
            ThemeChoice::Macchiato => Self::Macchiato,
            ThemeChoice::Mocha => Self::Mocha,
            ThemeChoice::WhiteDark => Self::WhiteDark,
            ThemeChoice::BlackLight => Self::BlackLight,
            ThemeChoice::BlueAccent => Self::BlueAccent,
            ThemeChoice::ButterCream => Self::ButterCream,
            ThemeChoice::MintCream => Self::MintCream,
            ThemeChoice::OliveGrove => Self::OliveGrove,
            ThemeChoice::VioletGrid => Self::VioletGrid,
            ThemeChoice::VioletNight => Self::VioletNight,
            ThemeChoice::Android13 => Self::Android13,
            ThemeChoice::Nord => Self::Nord,
            ThemeChoice::Dracula => Self::Dracula,
            ThemeChoice::Gruvbox => Self::Gruvbox,
            ThemeChoice::TokyoNight => Self::TokyoNight,
            ThemeChoice::Rose => Self::Rose,
            ThemeChoice::AmoledBlack => Self::AmoledBlack,
            ThemeChoice::OceanTeal => Self::OceanTeal,
            ThemeChoice::Sunset => Self::Sunset,
            ThemeChoice::Forest => Self::Forest,
            ThemeChoice::Sakura => Self::Sakura,
            ThemeChoice::Custom => Self::Custom,
        }
    }
}

impl From<ThemePreference> for ThemeChoice {
    fn from(preference: ThemePreference) -> Self {
        match preference {
            ThemePreference::System => Self::System,
            ThemePreference::Light => Self::Light,
            ThemePreference::Dark => Self::Dark,
            ThemePreference::Latte => Self::Latte,
            ThemePreference::Frappe => Self::Frappe,
            ThemePreference::Macchiato => Self::Macchiato,
            ThemePreference::Mocha => Self::Mocha,
            ThemePreference::WhiteDark => Self::WhiteDark,
            ThemePreference::BlackLight => Self::BlackLight,
            ThemePreference::BlueAccent => Self::BlueAccent,
            ThemePreference::ButterCream => Self::ButterCream,
            ThemePreference::MintCream => Self::MintCream,
            ThemePreference::OliveGrove => Self::OliveGrove,
            ThemePreference::VioletGrid => Self::VioletGrid,
            ThemePreference::VioletNight => Self::VioletNight,
            ThemePreference::Android13 => Self::Android13,
            ThemePreference::Nord => Self::Nord,
            ThemePreference::Dracula => Self::Dracula,
            ThemePreference::Gruvbox => Self::Gruvbox,
            ThemePreference::TokyoNight => Self::TokyoNight,
            ThemePreference::Rose => Self::Rose,
            ThemePreference::AmoledBlack => Self::AmoledBlack,
            ThemePreference::OceanTeal => Self::OceanTeal,
            ThemePreference::Sunset => Self::Sunset,
            ThemePreference::Forest => Self::Forest,
            ThemePreference::Sakura => Self::Sakura,
            ThemePreference::Custom => Self::Custom,
        }
    }
}

/// Filesystems that represent a real, user-visible partition.
const REAL_FILESYSTEMS: &[&str] = &[
    "ext2", "ext3", "ext4", "btrfs", "xfs", "vfat", "exfat", "ntfs", "ntfs3", "f2fs", "jfs",
    "reiserfs", "bcachefs", "zfs", "fuseblk",
];

#[derive(Debug, Clone)]
struct MountEntry {
    device: String,
    mount: String,
    fstype: String,
}

fn decode_mount_field(value: &str) -> String {
    // /proc/self/mounts escapes spaces, tabs, newlines and backslashes as \ooo.
    let mut out = String::with_capacity(value.len());
    let bytes = value.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 3 < bytes.len() {
            let octal = &value[i + 1..i + 4];
            if let Ok(byte) = u8::from_str_radix(octal, 8) {
                out.push(byte as char);
                i += 4;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn decode_label(value: &str) -> String {
    value
        .replace("\\x20", " ")
        .replace("\\x09", "\t")
        .replace("\\x0a", "\n")
        .replace("\\x5c", "\\")
}

fn read_mounts() -> Vec<MountEntry> {
    let Ok(contents) = fs::read_to_string("/proc/self/mounts") else {
        return Vec::new();
    };

    contents
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let device = decode_mount_field(fields.next()?);
            let mount = decode_mount_field(fields.next()?);
            let fstype = fields.next()?.to_string();

            if !device.starts_with("/dev/") {
                return None;
            }
            if device.starts_with("/dev/loop") || device.starts_with("/dev/zram") {
                return None;
            }
            if !REAL_FILESYSTEMS.contains(&fstype.as_str()) {
                return None;
            }

            Some(MountEntry {
                device,
                mount,
                fstype,
            })
        })
        .collect()
}

/// A btrfs disk can be mounted at several places (/, /home, /var, ...).
/// Keep one card per device, preferring the root mount.
fn one_per_device(entries: Vec<MountEntry>) -> Vec<MountEntry> {
    let mut by_device: HashMap<String, Vec<MountEntry>> = HashMap::new();
    for entry in entries {
        by_device.entry(entry.device.clone()).or_default().push(entry);
    }

    let mut result: Vec<MountEntry> = by_device
        .into_values()
        .filter_map(|mounts| mounts.into_iter().min_by_key(|m| mount_rank(&m.mount)))
        .collect();

    result.sort_by(|a, b| {
        mount_rank(&a.mount)
            .cmp(&mount_rank(&b.mount))
            .then_with(|| a.device.cmp(&b.device))
    });
    result
}

fn mount_rank(mount: &str) -> (u8, usize, usize) {
    match mount {
        "/" => (0, 0, 0),
        "/boot" => (1, 0, 0),
        _ => (2, mount.matches('/').count(), mount.len()),
    }
}

fn filesystem_labels() -> HashMap<String, String> {
    let mut labels = HashMap::new();

    for directory in ["/dev/disk/by-label", "/dev/disk/by-partlabel"] {
        let Ok(entries) = fs::read_dir(directory) else {
            continue;
        };

        for entry in entries.flatten() {
            let Some(name) = entry.file_name().to_str().map(decode_label) else {
                continue;
            };
            let Ok(target) = fs::canonicalize(entry.path()) else {
                continue;
            };
            labels
                .entry(target.to_string_lossy().into_owned())
                .or_insert(name);
        }
    }

    labels
}

fn disk_space(mount: &str) -> Option<(u64, u64)> {
    let path = CString::new(mount).ok()?;
    let mut stats: libc::statvfs = unsafe { std::mem::zeroed() };

    if unsafe { libc::statvfs(path.as_ptr(), &mut stats) } != 0 {
        return None;
    }

    let block_size = stats.f_frsize as u64;
    let total = stats.f_blocks as u64 * block_size;
    let free = stats.f_bavail as u64 * block_size;
    Some((total, free))
}

fn pretty_mount_name(mount: &str) -> String {
    match mount {
        "/" => "OS".to_string(),
        "/boot" => "Boot".to_string(),
        "/home" => "Home".to_string(),
        "/var" => "Var".to_string(),
        _ => {
            let last = mount
                .trim_end_matches('/')
                .rsplit('/')
                .next()
                .unwrap_or("Disk");
            let mut chars = last.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => "Disk".to_string(),
            }
        }
    }
}

fn format_size(bytes: u64) -> String {
    const MIB: f64 = 1024.0 * 1024.0;
    const GIB: f64 = 1024.0 * 1024.0 * 1024.0;

    let gib = bytes as f64 / GIB;
    if gib >= 1.0 {
        let value = format!("{gib:.1}");
        format!("{} GB", value.trim_end_matches(".0"))
    } else {
        format!("{:.0} MB", bytes as f64 / MIB)
    }
}

fn short_device(device: &str) -> &str {
    device.rsplit('/').next().unwrap_or(device)
}

fn collect_disks() -> Vec<DiskInfo> {
    let labels = filesystem_labels();

    one_per_device(read_mounts())
        .into_iter()
        .filter_map(|entry| {
            let (total, free) = disk_space(&entry.mount)?;
            if total == 0 {
                return None;
            }

            let used = ((total.saturating_sub(free)) as f64 / total as f64 * 100.0)
                .clamp(0.0, 100.0) as f32;

            let label = labels.get(&entry.device).cloned().or_else(|| {
                fs::canonicalize(Path::new(&entry.device))
                    .ok()
                    .and_then(|path| labels.get(&path.to_string_lossy().into_owned()).cloned())
            });

            let name = label.unwrap_or_else(|| pretty_mount_name(&entry.mount));
            let title = format!("{} ({})", name, short_device(&entry.device));

            Some(DiskInfo {
                title: title.into(),
                device: entry.device.into(),
                mount: entry.mount.into(),
                fs: entry.fstype.into(),
                total: format_size(total).into(),
                free: format!("{} free of {}", format_size(free), format_size(total)).into(),
                used,
            })
        })
        .collect()
}

/// Read a directory and return (state, entries).
/// If `path` is relative, it is resolved against `current_path`.
fn read_directory(path: &Path) -> (BrowseState, Vec<FileEntry>) {
    // Try to read the directory
    let read_result = fs::read_dir(path);

    match read_result {
        Err(e) => {
            use std::io::ErrorKind;
            match e.kind() {
                ErrorKind::PermissionDenied => (BrowseState::NoPermission, vec![]),
                _ => (BrowseState::NoPermission, vec![]),
            }
        }
        Ok(iter) => {
            let mut entries: Vec<FileEntry> = iter
                .filter_map(|e| e.ok())
                .filter_map(|e| {
                    let name = e.file_name().to_string_lossy().into_owned();
                    // Skip hidden files (starting with .)
                    // Uncomment the next line if you want to show hidden files:
                    // let _ = ();
                    let meta = e.metadata().ok()?;
                    let is_dir = meta.is_dir();
                    let size = if is_dir {
                        String::new()
                    } else {
                        format_size(meta.len())
                    };
                    Some(FileEntry {
                        name: name.into(),
                        is_dir,
                        size: size.into(),
                    })
                })
                .collect();

            if entries.is_empty() {
                (BrowseState::Empty, vec![])
            } else {
                // Sort: directories first, then files, both alphabetically
                entries.sort_by(|a, b| {
                    b.is_dir
                        .cmp(&a.is_dir)
                        .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                });
                (BrowseState::Directory, entries)
            }
        }
    }
}

/// Parse the hex colours typed on the Settings → Theme page.
fn parse_hex(value: &str) -> Option<Color> {
    let digits = value.trim().trim_start_matches('#');
    let expand = |hex: &str| -> Option<u8> {
        if hex.len() == 2 {
            u8::from_str_radix(hex, 16).ok()
        } else if hex.len() == 1 {
            u8::from_str_radix(&hex.repeat(2), 16).ok()
        } else {
            None
        }
    };
    let (r, g, b) = match digits.len() {
        3 => (
            expand(&digits[0..1])?,
            expand(&digits[1..2])?,
            expand(&digits[2..3])?,
        ),
        6 => (
            expand(&digits[0..2])?,
            expand(&digits[2..4])?,
            expand(&digits[4..6])?,
        ),
        _ => return None,
    };
    Some(Color::from_argb_u8(0xff, r, g, b))
}

fn color_to_hex(color: Color) -> String {
    let c = color.to_argb_u8();
    format!("#{:02x}{:02x}{:02x}", c.red, c.green, c.blue)
}

/// Turn the hex strings currently in the UI into the live custom colours.
/// A half-typed or invalid value keeps the previous valid colour.
fn apply_custom_hexes(ui: &AppWindow) {
    let theme = ui.global::<Theme>();

    let hex = theme.get_custom_bg_hex().to_string();
    theme.set_custom_bg(parse_hex(&hex).unwrap_or_else(|| theme.get_custom_bg()));
    let hex = theme.get_custom_sidebar_hex().to_string();
    theme.set_custom_sidebar(parse_hex(&hex).unwrap_or_else(|| theme.get_custom_sidebar()));
    let hex = theme.get_custom_ribbon_hex().to_string();
    theme.set_custom_ribbon(parse_hex(&hex).unwrap_or_else(|| theme.get_custom_ribbon()));
    let hex = theme.get_custom_field_hex().to_string();
    theme.set_custom_field(parse_hex(&hex).unwrap_or_else(|| theme.get_custom_field()));
    let hex = theme.get_custom_accent_hex().to_string();
    theme.set_custom_accent(parse_hex(&hex).unwrap_or_else(|| theme.get_custom_accent()));
    let hex = theme.get_custom_text_hex().to_string();
    theme.set_custom_text(parse_hex(&hex).unwrap_or_else(|| theme.get_custom_text()));
    let hex = theme.get_custom_text_secondary_hex().to_string();
    theme.set_custom_text_secondary(
        parse_hex(&hex).unwrap_or_else(|| theme.get_custom_text_secondary()),
    );
    let hex = theme.get_custom_border_hex().to_string();
    theme.set_custom_border(parse_hex(&hex).unwrap_or_else(|| theme.get_custom_border()));
    let hex = theme.get_custom_icon_tint_hex().to_string();
    theme.set_custom_icon_tint(parse_hex(&hex).unwrap_or_else(|| theme.get_custom_icon_tint()));
}

fn custom_from_ui(ui: &AppWindow) -> CustomColors {
    let theme = ui.global::<Theme>();
    let field = |hex: slint::SharedString, color: Color| -> String {
        let text = hex.to_string();
        if parse_hex(&text).is_some() {
            text
        } else {
            color_to_hex(color)
        }
    };
    CustomColors {
        bg: field(theme.get_custom_bg_hex(), theme.get_custom_bg()),
        sidebar: field(theme.get_custom_sidebar_hex(), theme.get_custom_sidebar()),
        ribbon: field(theme.get_custom_ribbon_hex(), theme.get_custom_ribbon()),
        field: field(theme.get_custom_field_hex(), theme.get_custom_field()),
        accent: field(theme.get_custom_accent_hex(), theme.get_custom_accent()),
        text: field(theme.get_custom_text_hex(), theme.get_custom_text()),
        text_secondary: field(
            theme.get_custom_text_secondary_hex(),
            theme.get_custom_text_secondary(),
        ),
        border: field(theme.get_custom_border_hex(), theme.get_custom_border()),
        icon_tint: field(theme.get_custom_icon_tint_hex(), theme.get_custom_icon_tint()),
    }
}

fn save_from_ui(ui: &AppWindow) {
    let preference = ui.global::<Theme>().get_preference();
    let config = Config {
        theme: ThemeChoice::from(preference),
        custom: custom_from_ui(ui),
    };
    if let Err(err) = config.save() {
        eprintln!("folder-lab: could not save settings: {err}");
    }
}

// ─── Navigation State ───────────────────────────────────────────────────────

struct NavState {
    history: Vec<PathBuf>,
    index: usize,
}

impl NavState {
    fn new() -> Self {
        Self {
            history: vec![],
            index: 0,
        }
    }

    fn current(&self) -> Option<&PathBuf> {
        if self.history.is_empty() {
            None
        } else {
            self.history.get(self.index)
        }
    }

    fn push(&mut self, path: PathBuf) {
        // Drop forward history when navigating to a new location
        self.history.truncate(self.index + 1);
        self.history.push(path);
        self.index = self.history.len() - 1;
    }

    fn can_back(&self) -> bool {
        self.index > 0
    }

    fn can_forward(&self) -> bool {
        self.index + 1 < self.history.len()
    }

    fn go_back(&mut self) -> Option<&PathBuf> {
        if self.can_back() {
            self.index -= 1;
            self.current()
        } else {
            None
        }
    }

    fn go_forward(&mut self) -> Option<&PathBuf> {
        if self.can_forward() {
            self.index += 1;
            self.current()
        } else {
            None
        }
    }
}

// ─── Apply a directory (or home) to the UI ──────────────────────────────────

fn apply_directory(ui: &AppWindow, path: &Path) {
    let (state, entries) = read_directory(path);
    ui.set_files(ModelRc::new(VecModel::from(entries)));
    ui.set_browse_state(state);
    ui.set_selected_index(-1);
    ui.set_current_path(path.to_string_lossy().as_ref().into());
    update_nav_buttons(ui);
}

fn apply_home(ui: &AppWindow) {
    ui.set_files(ModelRc::new(VecModel::from(vec![])));
    ui.set_browse_state(BrowseState::Home);
    ui.set_selected_index(-1);
    ui.set_current_path(SharedString::from(""));
    update_nav_buttons(ui);
}

fn update_nav_buttons(ui: &AppWindow) {
    // can-go-up: true whenever we're not at home AND path has a parent
    let path_str = ui.get_current_path().to_string();
    let can_up = !path_str.is_empty() && {
        let p = PathBuf::from(&path_str);
        p.parent().is_some()
    };
    ui.set_can_go_up(can_up);
    // back/forward are set from NavState, handled by callers
}

fn main() -> Result<(), slint::PlatformError> {
    let ui = AppWindow::new()?;
    ui.set_disks(ModelRc::new(VecModel::from(collect_disks())));
    ui.set_files(ModelRc::new(VecModel::from(vec![])));
    ui.set_browse_state(BrowseState::Home);

    // Restore the theme and the custom colours saved by the previous run.
    let saved = Config::load();
    let theme = ui.global::<Theme>();
    theme.set_preference(saved.theme.into());
    theme.set_custom_bg_hex(saved.custom.bg.as_str().into());
    theme.set_custom_sidebar_hex(saved.custom.sidebar.as_str().into());
    theme.set_custom_ribbon_hex(saved.custom.ribbon.as_str().into());
    theme.set_custom_field_hex(saved.custom.field.as_str().into());
    theme.set_custom_accent_hex(saved.custom.accent.as_str().into());
    theme.set_custom_text_hex(saved.custom.text.as_str().into());
    theme.set_custom_text_secondary_hex(saved.custom.text_secondary.as_str().into());
    theme.set_custom_border_hex(saved.custom.border.as_str().into());
    theme.set_custom_icon_tint_hex(saved.custom.icon_tint.as_str().into());
    apply_custom_hexes(&ui);

    // ── Navigation state (shared via Rc<RefCell<>>) ──────────────────────────
    use std::cell::RefCell;
    use std::rc::Rc;

    let nav = Rc::new(RefCell::new(NavState::new()));

    // ── navigate-to ──────────────────────────────────────────────────────────
    let ui_weak = ui.as_weak();
    let nav_clone = nav.clone();
    ui.on_navigate_to(move |raw_path| {
        let Some(ui) = ui_weak.upgrade() else { return };
        let raw = raw_path.to_string();

        let target: PathBuf = if raw.starts_with('/') {
            PathBuf::from(&raw)
        } else {
            // relative: resolve against current path
            let current_str = ui.get_current_path().to_string();
            if current_str.is_empty() {
                // at home: ignore relative paths (shouldn't happen)
                return;
            }
            PathBuf::from(&current_str).join(&raw)
        };

        // Canonicalize to resolve ".." and symlinks
        let resolved = target.canonicalize().unwrap_or(target);

        nav_clone.borrow_mut().push(resolved.clone());
        let nav_ref = nav_clone.borrow();
        ui.set_can_go_back(nav_ref.can_back());
        ui.set_can_go_forward(nav_ref.can_forward());
        drop(nav_ref);

        apply_directory(&ui, &resolved);
    });

    // ── navigate-home ─────────────────────────────────────────────────────────
    let ui_weak = ui.as_weak();
    let nav_clone = nav.clone();
    ui.on_navigate_home(move || {
        let Some(ui) = ui_weak.upgrade() else { return };
        // Push a sentinel so back works
        nav_clone.borrow_mut().push(PathBuf::from("__home__"));
        let nav_ref = nav_clone.borrow();
        ui.set_can_go_back(nav_ref.can_back());
        ui.set_can_go_forward(nav_ref.can_forward());
        drop(nav_ref);
        apply_home(&ui);
    });

    // ── navigate-up ───────────────────────────────────────────────────────────
    let ui_weak = ui.as_weak();
    let nav_clone = nav.clone();
    ui.on_navigate_up(move || {
        let Some(ui) = ui_weak.upgrade() else { return };
        let current_str = ui.get_current_path().to_string();
        if current_str.is_empty() {
            return;
        }
        let current = PathBuf::from(&current_str);
        if let Some(parent) = current.parent() {
            let parent = parent.to_path_buf();
            if parent.as_os_str().is_empty() {
                // went above root → go home
                nav_clone.borrow_mut().push(PathBuf::from("__home__"));
                let nav_ref = nav_clone.borrow();
                ui.set_can_go_back(nav_ref.can_back());
                ui.set_can_go_forward(nav_ref.can_forward());
                drop(nav_ref);
                apply_home(&ui);
            } else {
                nav_clone.borrow_mut().push(parent.clone());
                let nav_ref = nav_clone.borrow();
                ui.set_can_go_back(nav_ref.can_back());
                ui.set_can_go_forward(nav_ref.can_forward());
                drop(nav_ref);
                apply_directory(&ui, &parent);
            }
        } else {
            // Already at root – go home
            nav_clone.borrow_mut().push(PathBuf::from("__home__"));
            let nav_ref = nav_clone.borrow();
            ui.set_can_go_back(nav_ref.can_back());
            ui.set_can_go_forward(nav_ref.can_forward());
            drop(nav_ref);
            apply_home(&ui);
        }
    });

    // ── navigate-back ─────────────────────────────────────────────────────────
    let ui_weak = ui.as_weak();
    let nav_clone = nav.clone();
    ui.on_navigate_back(move || {
        let Some(ui) = ui_weak.upgrade() else { return };
        let mut nav_ref = nav_clone.borrow_mut();
        if let Some(path) = nav_ref.go_back().cloned() {
            let can_back = nav_ref.can_back();
            let can_forward = nav_ref.can_forward();
            drop(nav_ref);
            ui.set_can_go_back(can_back);
            ui.set_can_go_forward(can_forward);
            if path == PathBuf::from("__home__") {
                apply_home(&ui);
            } else {
                apply_directory(&ui, &path);
            }
        }
    });

    // ── navigate-forward ──────────────────────────────────────────────────────
    let ui_weak = ui.as_weak();
    let nav_clone = nav.clone();
    ui.on_navigate_forward(move || {
        let Some(ui) = ui_weak.upgrade() else { return };
        let mut nav_ref = nav_clone.borrow_mut();
        if let Some(path) = nav_ref.go_forward().cloned() {
            let can_back = nav_ref.can_back();
            let can_forward = nav_ref.can_forward();
            drop(nav_ref);
            ui.set_can_go_back(can_back);
            ui.set_can_go_forward(can_forward);
            if path == PathBuf::from("__home__") {
                apply_home(&ui);
            } else {
                apply_directory(&ui, &path);
            }
        }
    });

    // ── key-nav (letter navigation) ───────────────────────────────────────────
    let ui_weak = ui.as_weak();
    ui.on_key_nav(move |ch| {
        let Some(ui) = ui_weak.upgrade() else { return };
        if ui.get_browse_state() != BrowseState::Directory {
            return;
        }
        let ch_str = ch.to_string();
        if ch_str.is_empty() {
            return;
        }

        let current_sel = ui.get_selected_index();
        let files = ui.get_files();
        let count = files.row_count();
        if count == 0 {
            return;
        }

        let query = ch_str.to_lowercase();
        let mut matches = Vec::new();
        for i in 0..count {
            if let Some(entry) = files.row_data(i) {
                let name = entry.name.to_lowercase();
                if name.starts_with(&query) {
                    matches.push(i as i32);
                }
            }
        }

        if matches.is_empty() {
            return;
        }

        let next_index = if let Some(pos) = matches.iter().position(|&idx| idx == current_sel) {
            let next_pos = (pos + 1) % matches.len();
            matches[next_pos]
        } else {
            matches[0]
        };

        ui.set_selected_index(next_index);
    });

    // ── navigate-arrows ───────────────────────────────────────────────────────
    let ui_weak = ui.as_weak();
    ui.on_navigate_arrows(move |delta| {
        let Some(ui) = ui_weak.upgrade() else { return };
        if ui.get_browse_state() != BrowseState::Directory {
            return;
        }
        let count = ui.get_files().row_count() as i32;
        if count == 0 {
            return;
        }
        let current = ui.get_selected_index();
        let next = if current < 0 {
            0
        } else {
            (current + delta).clamp(0, count - 1)
        };
        ui.set_selected_index(next);
    });

    // ── activate-selected (Enter key) ─────────────────────────────────────────
    let ui_weak = ui.as_weak();
    let nav_clone = nav.clone();
    ui.on_activate_selected(move || {
        let Some(ui) = ui_weak.upgrade() else { return };
        if ui.get_browse_state() != BrowseState::Directory {
            return;
        }
        let sel = ui.get_selected_index();
        if sel < 0 {
            return;
        }
        let files = ui.get_files();
        if let Some(entry) = files.row_data(sel as usize) {
            if entry.is_dir {
                let current_str = ui.get_current_path().to_string();
                let target = PathBuf::from(&current_str).join(entry.name.to_string());
                let resolved = target.canonicalize().unwrap_or(target);

                nav_clone.borrow_mut().push(resolved.clone());
                let nav_ref = nav_clone.borrow();
                ui.set_can_go_back(nav_ref.can_back());
                ui.set_can_go_forward(nav_ref.can_forward());
                drop(nav_ref);

                apply_directory(&ui, &resolved);
            }
        }
    });

    // ── Theme ─────────────────────────────────────────────────────────────────
    let ui_weak = ui.as_weak();
    ui.on_theme_selected(move |preference| {
        let Some(ui) = ui_weak.upgrade() else { return };
        ui.global::<Theme>().set_preference(preference);
        save_from_ui(&ui);
    });

    let ui_weak = ui.as_weak();
    ui.on_custom_colors_changed(move || {
        let Some(ui) = ui_weak.upgrade() else { return };
        apply_custom_hexes(&ui);
        save_from_ui(&ui);
    });

    ui.run()
}
