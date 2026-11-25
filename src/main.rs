slint::include_modules!();

mod config;
mod fs_ops;
mod icon_theme;
mod nav;

use std::cell::RefCell;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use slint::{Color, ComponentHandle, Model, ModelRc, SharedString, VecModel};

use crate::config::{Config, CustomColors, ThemeChoice};
use crate::fs_ops::{
    collect_disks, compute_dir_stats, disk_space, format_count, format_size, get_file_details,
    read_directory, sort_entries,
};
use crate::icon_theme::IconTheme;
use crate::nav::{NavState, NavTarget};

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

// ─── Clipboard state for cut/copy/paste ──────────────────────────────────────

#[derive(Default)]
struct ClipboardState {
    paths: Vec<PathBuf>,
    is_cut: bool,
}

// ─── Apply a directory (or home) to the UI ──────────────────────────────────

fn empty_file_details() -> FileDetails {
    FileDetails {
        name: "".into(),
        is_dir: false,
        is_disk: false,
        file_type: "".into(),
        location: "".into(),
        date_modified: "".into(),
        size: "".into(),
        folders_count: "".into(),
        files_count: "".into(),
        free_space: "".into(),
        total_space: "".into(),
        used_space: "".into(),
        fs_type: "".into(),
        device: "".into(),
        disk_used_percent: 0.0,
        has_selection: false,
        icon: slint::Image::default(),
    }
}

fn get_disk_details(disk: &DiskInfo) -> FileDetails {
    let mount = disk.mount.to_string();
    let (total_str, free_str, used_str, used_pct) = if let Some((total, free)) = disk_space(&mount) {
        let used = total.saturating_sub(free);
        let pct = if total > 0 {
            ((used as f64 / total as f64) * 100.0).clamp(0.0, 100.0) as f32
        } else {
            disk.used
        };
        (format_size(total), format_size(free), format_size(used), pct)
    } else {
        (disk.total.to_string(), "-".to_string(), "-".to_string(), disk.used)
    };

    let fs_display = disk.fs.to_string();
    let file_type = if fs_display.is_empty() {
        "Local Disk".to_string()
    } else {
        format!("Local Disk ({})", fs_display)
    };

    FileDetails {
        name: disk.title.clone(),
        is_dir: false,
        is_disk: true,
        file_type: file_type.into(),
        location: disk.mount.clone(),
        date_modified: "-".into(),
        size: total_str.clone().into(),
        folders_count: "".into(),
        files_count: "".into(),
        free_space: free_str.into(),
        total_space: total_str.into(),
        used_space: used_str.into(),
        fs_type: fs_display.into(),
        device: disk.device.clone(),
        disk_used_percent: used_pct,
        has_selection: true,
        icon: disk.icon.clone(),
    }
}

fn apply_directory(
    ui: &AppWindow,
    path: &Path,
    show_hidden: bool,
    folder_calc_gen: &Arc<AtomicU64>,
    icon_theme: &IconTheme,
) {
    let sort_field = ui.get_sort_field();
    let sort_order = ui.get_sort_order();
    let (state, entries) = read_directory(path, show_hidden, sort_field, sort_order, icon_theme);
    ui.set_files(ModelRc::new(VecModel::from(entries)));
    ui.set_browse_state(state);
    ui.set_selected_index(-1);
    ui.set_current_path(path.to_string_lossy().as_ref().into());
    update_details_for_selection(ui, folder_calc_gen, icon_theme);
    update_nav_buttons(ui);
}

fn apply_home(ui: &AppWindow, folder_calc_gen: &Arc<AtomicU64>, icon_theme: &IconTheme) {
    ui.set_files(ModelRc::new(VecModel::from(vec![])));
    ui.set_browse_state(BrowseState::Home);
    ui.set_selected_index(-1);
    ui.set_current_path(SharedString::from(""));
    update_details_for_selection(ui, folder_calc_gen, icon_theme);
    update_nav_buttons(ui);
}

fn update_nav_buttons(ui: &AppWindow) {
    let path_str = ui.get_current_path().to_string();
    let can_up = !path_str.is_empty() && {
        let p = PathBuf::from(&path_str);
        p.parent().is_some()
    };
    ui.set_can_go_up(can_up);
}

fn update_details_for_selection(
    ui: &AppWindow,
    folder_calc_gen: &Arc<AtomicU64>,
    icon_theme: &IconTheme,
) {
    folder_calc_gen.fetch_add(1, Ordering::SeqCst);

    let sel = ui.get_selected_index();
    if sel < 0 {
        ui.set_selected_details(empty_file_details());
        return;
    }

    match ui.get_browse_state() {
        BrowseState::Home => {
            let disks = ui.get_disks();
            if let Some(disk) = disks.row_data(sel as usize) {
                let details = get_disk_details(&disk);
                ui.set_selected_details(details);
            } else {
                ui.set_selected_details(empty_file_details());
            }
        }
        BrowseState::Directory => {
            let files = ui.get_files();
            if let Some(entry) = files.row_data(sel as usize) {
                let current_str = ui.get_current_path().to_string();
                let item_path = if current_str.is_empty() {
                    PathBuf::from(entry.name.to_string())
                } else {
                    PathBuf::from(&current_str).join(entry.name.to_string())
                };
                let details = get_file_details(&item_path, icon_theme);
                let is_dir = details.is_dir;
                ui.set_selected_details(details);

                if is_dir {
                    let gen = folder_calc_gen.load(Ordering::SeqCst);
                    let gen_clone = folder_calc_gen.clone();
                    let ui_weak = ui.as_weak();
                    let path_clone = item_path.clone();

                    std::thread::spawn(move || {
                        let stats = compute_dir_stats(&path_clone, || {
                            gen_clone.load(Ordering::SeqCst) != gen
                        });

                        if gen_clone.load(Ordering::SeqCst) == gen {
                            let _ = ui_weak.upgrade_in_event_loop(move |ui| {
                                if ui.get_selected_index() == sel {
                                    let mut cur_details = ui.get_selected_details();
                                    if cur_details.has_selection && cur_details.is_dir {
                                        if let Some(stats) = stats {
                                            cur_details.size = format_size(stats.total_bytes).into();
                                            cur_details.folders_count = format_count(stats.dir_count).into();
                                            cur_details.files_count = format_count(stats.file_count).into();
                                        } else {
                                            cur_details.size = "-".into();
                                            cur_details.folders_count = "-".into();
                                            cur_details.files_count = "-".into();
                                        }
                                        ui.set_selected_details(cur_details);
                                    }
                                }
                            });
                        }
                    });
                }
            } else {
                ui.set_selected_details(empty_file_details());
            }
        }
        _ => {
            ui.set_selected_details(empty_file_details());
        }
    }
}

fn resolve_special_path(kind: &str) -> PathBuf {
    let home = env::var("HOME").unwrap_or_else(|_| "/home".to_string());
    let home_path = PathBuf::from(&home);
    match kind {
        "desktop" => {
            let p = home_path.join("Desktop");
            if p.exists() { p } else { home_path }
        }
        "downloads" => {
            let p = home_path.join("Downloads");
            if p.exists() { p } else { home_path }
        }
        "documents" => {
            let p = home_path.join("Documents");
            if p.exists() { p } else { home_path }
        }
        "pictures" => {
            let p = home_path.join("Pictures");
            if p.exists() { p } else { home_path }
        }
        "music" => {
            let p = home_path.join("Music");
            if p.exists() { p } else { home_path }
        }
        "videos" => {
            let p = home_path.join("Videos");
            if p.exists() { p } else { home_path }
        }
        _ => home_path,
    }
}

fn main() -> Result<(), slint::PlatformError> {
    let ui = AppWindow::new()?;
    let icon_theme = Rc::new(IconTheme::new());

    ui.set_disks(ModelRc::new(VecModel::from(collect_disks(&icon_theme))));
    ui.set_files(ModelRc::new(VecModel::from(vec![])));
    ui.set_browse_state(BrowseState::Home);
    ui.set_view_mode(ViewMode::MediumIcons);
    ui.set_sort_field(SortField::Name);
    ui.set_sort_order(SortOrder::Ascending);
    ui.set_show_details(true);

    // Restore the theme and custom colours
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

    // ── CLI Argument & Initial Navigation Target ─────────────────────────────
    let cli_arg = env::args().nth(1);
    let initial_target = if let Some(ref path_str) = cli_arg {
        let path = PathBuf::from(path_str);
        if path.is_dir() {
            let canonical = path.canonicalize().unwrap_or(path);
            NavTarget::Directory(canonical)
        } else {
            NavTarget::Home
        }
    } else {
        NavTarget::Home
    };

    let show_hidden = Rc::new(RefCell::new(false));
    let clipboard = Rc::new(RefCell::new(ClipboardState::default()));
    let nav = Rc::new(RefCell::new(NavState::new(initial_target.clone())));
    let folder_calc_gen = Arc::new(AtomicU64::new(0));

    // Apply the initial target to the UI
    match &initial_target {
        NavTarget::Home => apply_home(&ui, &folder_calc_gen, &icon_theme),
        NavTarget::Directory(p) => apply_directory(&ui, p, *show_hidden.borrow(), &folder_calc_gen, &icon_theme),
    }
    ui.set_can_go_back(false);
    ui.set_can_go_forward(false);

    // ── selection-changed ────────────────────────────────────────────────────
    let ui_weak = ui.as_weak();
    let folder_calc_gen_clone = folder_calc_gen.clone();
    let icon_theme_clone = icon_theme.clone();
    ui.on_selection_changed(move |_sel| {
        let Some(ui) = ui_weak.upgrade() else { return };
        update_details_for_selection(&ui, &folder_calc_gen_clone, &icon_theme_clone);
    });

    // ── navigate-to ──────────────────────────────────────────────────────────
    let ui_weak = ui.as_weak();
    let nav_clone = nav.clone();
    let show_hidden_clone = show_hidden.clone();
    let folder_calc_gen_clone = folder_calc_gen.clone();
    let icon_theme_clone = icon_theme.clone();
    ui.on_navigate_to(move |raw_path| {
        let Some(ui) = ui_weak.upgrade() else { return };
        let raw = raw_path.to_string();

        let target: PathBuf = if raw.starts_with('/') {
            PathBuf::from(&raw)
        } else {
            let current_str = ui.get_current_path().to_string();
            if current_str.is_empty() {
                return;
            }
            PathBuf::from(&current_str).join(&raw)
        };

        let resolved = target.canonicalize().unwrap_or(target);

        nav_clone.borrow_mut().push(NavTarget::Directory(resolved.clone()));
        let nav_ref = nav_clone.borrow();
        ui.set_can_go_back(nav_ref.can_back());
        ui.set_can_go_forward(nav_ref.can_forward());
        drop(nav_ref);

        apply_directory(&ui, &resolved, *show_hidden_clone.borrow(), &folder_calc_gen_clone, &icon_theme_clone);
    });

    // ── navigate-special (Desktop, Downloads, etc.) ──────────────────────────
    let ui_weak = ui.as_weak();
    let nav_clone = nav.clone();
    let show_hidden_clone = show_hidden.clone();
    let folder_calc_gen_clone = folder_calc_gen.clone();
    let icon_theme_clone = icon_theme.clone();
    ui.on_navigate_special(move |kind| {
        let Some(ui) = ui_weak.upgrade() else { return };
        let target = resolve_special_path(&kind);
        let resolved = target.canonicalize().unwrap_or(target);

        nav_clone.borrow_mut().push(NavTarget::Directory(resolved.clone()));
        let nav_ref = nav_clone.borrow();
        ui.set_can_go_back(nav_ref.can_back());
        ui.set_can_go_forward(nav_ref.can_forward());
        drop(nav_ref);

        apply_directory(&ui, &resolved, *show_hidden_clone.borrow(), &folder_calc_gen_clone, &icon_theme_clone);
    });

    // ── navigate-home (This PC) ──────────────────────────────────────────────
    let ui_weak = ui.as_weak();
    let nav_clone = nav.clone();
    let folder_calc_gen_clone = folder_calc_gen.clone();
    let icon_theme_clone = icon_theme.clone();
    ui.on_navigate_home(move || {
        let Some(ui) = ui_weak.upgrade() else { return };
        nav_clone.borrow_mut().push(NavTarget::Home);
        let nav_ref = nav_clone.borrow();
        ui.set_can_go_back(nav_ref.can_back());
        ui.set_can_go_forward(nav_ref.can_forward());
        drop(nav_ref);
        apply_home(&ui, &folder_calc_gen_clone, &icon_theme_clone);
    });

    // ── navigate-up ───────────────────────────────────────────────────────────
    let ui_weak = ui.as_weak();
    let nav_clone = nav.clone();
    let show_hidden_clone = show_hidden.clone();
    let folder_calc_gen_clone = folder_calc_gen.clone();
    let icon_theme_clone = icon_theme.clone();
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
                nav_clone.borrow_mut().push(NavTarget::Home);
                let nav_ref = nav_clone.borrow();
                ui.set_can_go_back(nav_ref.can_back());
                ui.set_can_go_forward(nav_ref.can_forward());
                drop(nav_ref);
                apply_home(&ui, &folder_calc_gen_clone, &icon_theme_clone);
            } else {
                nav_clone.borrow_mut().push(NavTarget::Directory(parent.clone()));
                let nav_ref = nav_clone.borrow();
                ui.set_can_go_back(nav_ref.can_back());
                ui.set_can_go_forward(nav_ref.can_forward());
                drop(nav_ref);
                apply_directory(&ui, &parent, *show_hidden_clone.borrow(), &folder_calc_gen_clone, &icon_theme_clone);
            }
        } else {
            nav_clone.borrow_mut().push(NavTarget::Home);
            let nav_ref = nav_clone.borrow();
            ui.set_can_go_back(nav_ref.can_back());
            ui.set_can_go_forward(nav_ref.can_forward());
            drop(nav_ref);
            apply_home(&ui, &folder_calc_gen_clone, &icon_theme_clone);
        }
    });

    // ── navigate-back ─────────────────────────────────────────────────────────
    let ui_weak = ui.as_weak();
    let nav_clone = nav.clone();
    let show_hidden_clone = show_hidden.clone();
    let folder_calc_gen_clone = folder_calc_gen.clone();
    let icon_theme_clone = icon_theme.clone();
    ui.on_navigate_back(move || {
        let Some(ui) = ui_weak.upgrade() else { return };
        let mut nav_ref = nav_clone.borrow_mut();
        if let Some(target) = nav_ref.go_back().cloned() {
            let can_back = nav_ref.can_back();
            let can_forward = nav_ref.can_forward();
            drop(nav_ref);
            ui.set_can_go_back(can_back);
            ui.set_can_go_forward(can_forward);
            match target {
                NavTarget::Home => apply_home(&ui, &folder_calc_gen_clone, &icon_theme_clone),
                NavTarget::Directory(path) => apply_directory(&ui, &path, *show_hidden_clone.borrow(), &folder_calc_gen_clone, &icon_theme_clone),
            }
        }
    });

    // ── navigate-forward ──────────────────────────────────────────────────────
    let ui_weak = ui.as_weak();
    let nav_clone = nav.clone();
    let show_hidden_clone = show_hidden.clone();
    let folder_calc_gen_clone = folder_calc_gen.clone();
    let icon_theme_clone = icon_theme.clone();
    ui.on_navigate_forward(move || {
        let Some(ui) = ui_weak.upgrade() else { return };
        let mut nav_ref = nav_clone.borrow_mut();
        if let Some(target) = nav_ref.go_forward().cloned() {
            let can_back = nav_ref.can_back();
            let can_forward = nav_ref.can_forward();
            drop(nav_ref);
            ui.set_can_go_back(can_back);
            ui.set_can_go_forward(can_forward);
            match target {
                NavTarget::Home => apply_home(&ui, &folder_calc_gen_clone, &icon_theme_clone),
                NavTarget::Directory(path) => apply_directory(&ui, &path, *show_hidden_clone.borrow(), &folder_calc_gen_clone, &icon_theme_clone),
            }
        }
    });

    // ── refresh ───────────────────────────────────────────────────────────────
    let ui_weak = ui.as_weak();
    let show_hidden_clone = show_hidden.clone();
    let folder_calc_gen_clone = folder_calc_gen.clone();
    let icon_theme_clone = icon_theme.clone();
    ui.on_refresh(move || {
        let Some(ui) = ui_weak.upgrade() else { return };
        let current_str = ui.get_current_path().to_string();
        if current_str.is_empty() {
            ui.set_disks(ModelRc::new(VecModel::from(collect_disks(&icon_theme_clone))));
            apply_home(&ui, &folder_calc_gen_clone, &icon_theme_clone);
        } else {
            let path = PathBuf::from(&current_str);
            apply_directory(&ui, &path, *show_hidden_clone.borrow(), &folder_calc_gen_clone, &icon_theme_clone);
        }
    });

    // ── key-nav (letter navigation in directory and home) ─────────────────────
    let ui_weak = ui.as_weak();
    let folder_calc_gen_clone = folder_calc_gen.clone();
    let icon_theme_clone = icon_theme.clone();
    ui.on_key_nav(move |ch| {
        let Some(ui) = ui_weak.upgrade() else { return };
        let ch_str = ch.to_string();
        if ch_str.is_empty() {
            return;
        }

        let query = ch_str.to_lowercase();
        let current_sel = ui.get_selected_index();

        match ui.get_browse_state() {
            BrowseState::Home => {
                let disks = ui.get_disks();
                let count = disks.row_count();
                if count == 0 {
                    return;
                }
                let mut matches = Vec::new();
                for i in 0..count {
                    if let Some(disk) = disks.row_data(i) {
                        let name = disk.title.to_lowercase();
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
                update_details_for_selection(&ui, &folder_calc_gen_clone, &icon_theme_clone);
            }
            BrowseState::Directory => {
                let files = ui.get_files();
                let count = files.row_count();
                if count == 0 {
                    return;
                }
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
                update_details_for_selection(&ui, &folder_calc_gen_clone, &icon_theme_clone);
            }
            _ => {}
        }
    });

    // ── navigate-arrows ───────────────────────────────────────────────────────
    let ui_weak = ui.as_weak();
    let folder_calc_gen_clone = folder_calc_gen.clone();
    let icon_theme_clone = icon_theme.clone();
    ui.on_navigate_arrows(move |delta| {
        let Some(ui) = ui_weak.upgrade() else { return };
        let count = match ui.get_browse_state() {
            BrowseState::Home => ui.get_disks().row_count() as i32,
            BrowseState::Directory => ui.get_files().row_count() as i32,
            _ => 0,
        };
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
        update_details_for_selection(&ui, &folder_calc_gen_clone, &icon_theme_clone);
    });

    // ── activate-selected (Enter key) ─────────────────────────────────────────
    let ui_weak = ui.as_weak();
    let nav_clone = nav.clone();
    let show_hidden_clone = show_hidden.clone();
    let folder_calc_gen_clone = folder_calc_gen.clone();
    let icon_theme_clone = icon_theme.clone();
    ui.on_activate_selected(move || {
        let Some(ui) = ui_weak.upgrade() else { return };
        let sel = ui.get_selected_index();
        if sel < 0 {
            return;
        }

        if ui.get_browse_state() == BrowseState::Home {
            let disks = ui.get_disks();
            if let Some(disk) = disks.row_data(sel as usize) {
                let target = PathBuf::from(disk.mount.to_string());
                let resolved = target.canonicalize().unwrap_or(target);

                nav_clone.borrow_mut().push(NavTarget::Directory(resolved.clone()));
                let nav_ref = nav_clone.borrow();
                ui.set_can_go_back(nav_ref.can_back());
                ui.set_can_go_forward(nav_ref.can_forward());
                drop(nav_ref);

                apply_directory(&ui, &resolved, *show_hidden_clone.borrow(), &folder_calc_gen_clone, &icon_theme_clone);
            }
            return;
        }

        if ui.get_browse_state() == BrowseState::Directory {
            let files = ui.get_files();
            if let Some(entry) = files.row_data(sel as usize) {
                if entry.is_dir {
                    let current_str = ui.get_current_path().to_string();
                    let target = PathBuf::from(&current_str).join(entry.name.to_string());
                    let resolved = target.canonicalize().unwrap_or(target);

                    nav_clone.borrow_mut().push(NavTarget::Directory(resolved.clone()));
                    let nav_ref = nav_clone.borrow();
                    ui.set_can_go_back(nav_ref.can_back());
                    ui.set_can_go_forward(nav_ref.can_forward());
                    drop(nav_ref);

                    apply_directory(&ui, &resolved, *show_hidden_clone.borrow(), &folder_calc_gen_clone, &icon_theme_clone);
                }
            }
        }
    });

    // ── Sorting ───────────────────────────────────────────────────────────────
    let ui_weak = ui.as_weak();
    let folder_calc_gen_clone = folder_calc_gen.clone();
    let icon_theme_clone = icon_theme.clone();
    ui.on_sort_changed(move |field, order| {
        let Some(ui) = ui_weak.upgrade() else { return };
        if ui.get_browse_state() != BrowseState::Directory {
            return;
        }
        let files_rc = ui.get_files();
        let mut entries: Vec<FileEntry> = (0..files_rc.row_count())
            .filter_map(|i| files_rc.row_data(i))
            .collect();
        sort_entries(&mut entries, field, order);
        ui.set_files(ModelRc::new(VecModel::from(entries)));
        ui.set_selected_index(-1);
        update_details_for_selection(&ui, &folder_calc_gen_clone, &icon_theme_clone);
    });

    // ── View Mode changed ─────────────────────────────────────────────────────
    let ui_weak = ui.as_weak();
    ui.on_view_mode_changed(move |mode| {
        let Some(ui) = ui_weak.upgrade() else { return };
        ui.set_view_mode(mode);
    });

    // ── New folder action ─────────────────────────────────────────────────────
    let ui_weak = ui.as_weak();
    let show_hidden_clone = show_hidden.clone();
    let folder_calc_gen_clone = folder_calc_gen.clone();
    let icon_theme_clone = icon_theme.clone();
    ui.on_new_folder(move || {
        let Some(ui) = ui_weak.upgrade() else { return };
        let current_str = ui.get_current_path().to_string();
        if current_str.is_empty() {
            return;
        }
        let base_dir = PathBuf::from(&current_str);
        let mut folder_name = "New folder".to_string();
        let mut counter = 2;
        while base_dir.join(&folder_name).exists() {
            folder_name = format!("New folder ({counter})");
            counter += 1;
        }
        let _ = fs::create_dir(base_dir.join(&folder_name));
        apply_directory(&ui, &base_dir, *show_hidden_clone.borrow(), &folder_calc_gen_clone, &icon_theme_clone);
    });

    // ── Delete action ─────────────────────────────────────────────────────────
    let ui_weak = ui.as_weak();
    let show_hidden_clone = show_hidden.clone();
    let folder_calc_gen_clone = folder_calc_gen.clone();
    let icon_theme_clone = icon_theme.clone();
    ui.on_delete(move || {
        let Some(ui) = ui_weak.upgrade() else { return };
        let sel = ui.get_selected_index();
        if sel < 0 {
            return;
        }
        let files = ui.get_files();
        if let Some(entry) = files.row_data(sel as usize) {
            let current_str = ui.get_current_path().to_string();
            let path = PathBuf::from(&current_str).join(entry.name.to_string());
            if entry.is_dir {
                let _ = fs::remove_dir_all(&path);
            } else {
                let _ = fs::remove_file(&path);
            }
            apply_directory(&ui, &PathBuf::from(&current_str), *show_hidden_clone.borrow(), &folder_calc_gen_clone, &icon_theme_clone);
        }
    });

    // ── Cut / Copy / Paste actions ────────────────────────────────────────────
    let ui_weak = ui.as_weak();
    let clip_clone = clipboard.clone();
    ui.on_copy(move || {
        let Some(ui) = ui_weak.upgrade() else { return };
        let sel = ui.get_selected_index();
        if sel < 0 { return; }
        if let Some(entry) = ui.get_files().row_data(sel as usize) {
            let path = PathBuf::from(ui.get_current_path().to_string()).join(entry.name.to_string());
            let mut clip = clip_clone.borrow_mut();
            clip.paths = vec![path];
            clip.is_cut = false;
        }
    });

    let ui_weak = ui.as_weak();
    let clip_clone = clipboard.clone();
    ui.on_cut(move || {
        let Some(ui) = ui_weak.upgrade() else { return };
        let sel = ui.get_selected_index();
        if sel < 0 { return; }
        if let Some(entry) = ui.get_files().row_data(sel as usize) {
            let path = PathBuf::from(ui.get_current_path().to_string()).join(entry.name.to_string());
            let mut clip = clip_clone.borrow_mut();
            clip.paths = vec![path];
            clip.is_cut = true;
        }
    });

    let ui_weak = ui.as_weak();
    let clip_clone = clipboard.clone();
    let show_hidden_clone = show_hidden.clone();
    let folder_calc_gen_clone = folder_calc_gen.clone();
    let icon_theme_clone = icon_theme.clone();
    ui.on_paste(move || {
        let Some(ui) = ui_weak.upgrade() else { return };
        let current_str = ui.get_current_path().to_string();
        if current_str.is_empty() { return; }
        let dest_dir = PathBuf::from(&current_str);

        let mut clip = clip_clone.borrow_mut();
        for src in &clip.paths {
            if let Some(name) = src.file_name() {
                let dest = dest_dir.join(name);
                if clip.is_cut {
                    let _ = fs::rename(src, &dest);
                } else if src.is_file() {
                    let _ = fs::copy(src, &dest);
                }
            }
        }
        if clip.is_cut {
            clip.paths.clear();
        }
        drop(clip);

        apply_directory(&ui, &dest_dir, *show_hidden_clone.borrow(), &folder_calc_gen_clone, &icon_theme_clone);
    });

    // ── Toggle Hidden files ───────────────────────────────────────────────────
    let ui_weak = ui.as_weak();
    let show_hidden_clone = show_hidden.clone();
    let folder_calc_gen_clone = folder_calc_gen.clone();
    let icon_theme_clone = icon_theme.clone();
    ui.on_toggle_hidden(move || {
        let Some(ui) = ui_weak.upgrade() else { return };
        let mut val = show_hidden_clone.borrow_mut();
        *val = !*val;
        let is_hidden = *val;
        drop(val);

        let current_str = ui.get_current_path().to_string();
        if !current_str.is_empty() {
            apply_directory(&ui, &PathBuf::from(&current_str), is_hidden, &folder_calc_gen_clone, &icon_theme_clone);
        }
    });

    // ── Theme Selection ───────────────────────────────────────────────────────
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
