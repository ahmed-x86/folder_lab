slint::include_modules!();

mod config;

use std::collections::HashMap;
use std::ffi::CString;
use std::fs;
use std::path::Path;

use slint::{ComponentHandle, ModelRc, VecModel};

use crate::config::{Config, ThemeChoice};

impl From<ThemeChoice> for ThemePreference {
    fn from(choice: ThemeChoice) -> Self {
        match choice {
            ThemeChoice::System => Self::System,
            ThemeChoice::Light => Self::Light,
            ThemeChoice::Dark => Self::Dark,
        }
    }
}

impl From<ThemePreference> for ThemeChoice {
    fn from(preference: ThemePreference) -> Self {
        match preference {
            ThemePreference::System => Self::System,
            ThemePreference::Light => Self::Light,
            ThemePreference::Dark => Self::Dark,
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

fn main() -> Result<(), slint::PlatformError> {
    let ui = AppWindow::new()?;
    ui.set_disks(ModelRc::new(VecModel::from(collect_disks())));

    // Restore the theme saved by the previous run.
    let saved_theme = ThemePreference::from(Config::load().theme);
    ui.global::<Theme>().set_preference(saved_theme);

    // Settings page: picking a theme is applied and remembered for the next run.
    ui.on_theme_selected(move |preference| {
        let config = Config {
            theme: ThemeChoice::from(preference),
        };
        if let Err(err) = config.save() {
            eprintln!("folder-lab: could not save settings: {err}");
        }
    });

    ui.run()
}
