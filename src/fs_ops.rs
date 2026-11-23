use std::collections::HashMap;
use std::ffi::CString;
use std::fs;
use std::path::Path;
use std::time::UNIX_EPOCH;

use crate::{DiskInfo, FileDetails, FileEntry, BrowseState, SortField, SortOrder};

/// Filesystems that represent a real, user-visible partition.
const REAL_FILESYSTEMS: &[&str] = &[
    "ext2", "ext3", "ext4", "btrfs", "xfs", "vfat", "exfat", "ntfs", "ntfs3", "f2fs", "jfs",
    "reiserfs", "bcachefs", "zfs", "fuseblk",
];

#[derive(Debug, Clone)]
pub struct MountEntry {
    pub device: String,
    pub mount: String,
    pub fstype: String,
}

pub fn decode_mount_field(value: &str) -> String {
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

pub fn decode_label(value: &str) -> String {
    value
        .replace("\\x20", " ")
        .replace("\\x09", "\t")
        .replace("\\x0a", "\n")
        .replace("\\x5c", "\\")
}

pub fn read_mounts() -> Vec<MountEntry> {
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

pub fn one_per_device(entries: Vec<MountEntry>) -> Vec<MountEntry> {
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

pub fn filesystem_labels() -> HashMap<String, String> {
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

pub fn disk_space(mount: &str) -> Option<(u64, u64)> {
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

pub fn pretty_mount_name(mount: &str) -> String {
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

pub fn format_size(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = 1024.0 * 1024.0;
    const GIB: f64 = 1024.0 * 1024.0 * 1024.0;

    let gib = bytes as f64 / GIB;
    if gib >= 1.0 {
        let value = format!("{gib:.1}");
        format!("{} GB", value.trim_end_matches(".0"))
    } else {
        let mib = bytes as f64 / MIB;
        if mib >= 1.0 {
            format!("{:.1} MB", mib)
        } else {
            let kib = bytes as f64 / KIB;
            if kib >= 1.0 {
                format!("{:.0} KB", kib)
            } else {
                format!("{} B", bytes)
            }
        }
    }
}

pub fn short_device(device: &str) -> &str {
    device.rsplit('/').next().unwrap_or(device)
}

pub fn collect_disks() -> Vec<DiskInfo> {
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

pub fn format_system_time(time: std::time::SystemTime) -> (String, i64) {
    let Ok(duration) = time.duration_since(UNIX_EPOCH) else {
        return ("-".to_string(), 0);
    };
    let secs = duration.as_secs() as i64;
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe {
        libc::localtime_r(&secs, &mut tm);
    }
    let mut buf = [0u8; 64];
    let len = unsafe {
        libc::strftime(
            buf.as_mut_ptr() as *mut libc::c_char,
            buf.len(),
            b"%d/%m/%Y %H:%M\0".as_ptr() as *const libc::c_char,
            &tm,
        )
    };
    if len > 0 {
        let s = String::from_utf8_lossy(&buf[..len]).into_owned();
        (s, secs)
    } else {
        ("-".to_string(), secs)
    }
}

pub fn detect_file_type(path: &Path, is_dir: bool) -> &'static str {
    if is_dir {
        return "File folder";
    }
    let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
        return "File";
    };
    match ext.to_lowercase().as_str() {
        "txt" => "Text Document",
        "pdf" => "PDF Document",
        "doc" | "docx" => "Microsoft Word Document",
        "xls" | "xlsx" => "Microsoft Excel Spreadsheet",
        "ppt" | "pptx" => "Microsoft PowerPoint Presentation",
        "png" => "PNG Image",
        "jpg" | "jpeg" => "JPEG Image",
        "gif" => "GIF Image",
        "svg" => "SVG Document",
        "webp" => "WEBP Image",
        "mp3" | "wav" | "flac" | "aac" | "ogg" => "Audio File",
        "mp4" | "mkv" | "avi" | "webm" | "mov" => "Video File",
        "zip" | "tar" | "gz" | "xz" | "7z" | "bz2" => "Archive File",
        "rs" => "Rust Source File",
        "py" => "Python Source File",
        "js" | "mjs" | "cjs" => "JavaScript File",
        "ts" => "TypeScript File",
        "json" => "JSON File",
        "toml" => "TOML File",
        "yaml" | "yml" => "YAML File",
        "sh" | "bash" | "zsh" => "Shell Script",
        "html" | "htm" => "HTML Document",
        "css" => "Cascading Style Sheet",
        "exe" | "bin" => "Application",
        "iso" | "img" => "Disk Image",
        "slint" => "Slint Design File",
        _ => "File",
    }
}

pub fn read_directory(
    path: &Path,
    show_hidden: bool,
    sort_field: SortField,
    sort_order: SortOrder,
) -> (BrowseState, Vec<FileEntry>) {
    let read_result = fs::read_dir(path);

    match read_result {
        Err(_) => (BrowseState::NoPermission, vec![]),
        Ok(iter) => {
            let mut entries: Vec<FileEntry> = iter
                .filter_map(|e| e.ok())
                .filter_map(|e| {
                    let name = e.file_name().to_string_lossy().into_owned();
                    let is_hidden = name.starts_with('.');
                    if is_hidden && !show_hidden {
                        return None;
                    }
                    let meta = e.metadata().ok()?;
                    let is_dir = meta.is_dir();
                    let len = meta.len();
                    let size_str = if is_dir {
                        String::new()
                    } else {
                        format_size(len)
                    };
                    let (date_str, date_secs) = meta
                        .modified()
                        .ok()
                        .map(format_system_time)
                        .unwrap_or_else(|| ("-".to_string(), 0));

                    let file_type = detect_file_type(&e.path(), is_dir);

                    Some(FileEntry {
                        name: name.into(),
                        is_dir,
                        size: size_str.into(),
                        size_bytes: len as i32,
                        date_modified: date_str.into(),
                        date_timestamp: date_secs as i32,
                        file_type: file_type.into(),
                        is_hidden,
                    })
                })
                .collect();

            if entries.is_empty() {
                (BrowseState::Empty, vec![])
            } else {
                sort_entries(&mut entries, sort_field, sort_order);
                (BrowseState::Directory, entries)
            }
        }
    }
}

pub fn sort_entries(entries: &mut [FileEntry], field: SortField, order: SortOrder) {
    entries.sort_by(|a, b| {
        // Folders always come first in file managers
        let dir_cmp = b.is_dir.cmp(&a.is_dir);
        if dir_cmp != std::cmp::Ordering::Equal {
            return dir_cmp;
        }

        let ordering = match field {
            SortField::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            SortField::Date => a.date_timestamp.cmp(&b.date_timestamp),
            SortField::Type => a.file_type.to_lowercase().cmp(&b.file_type.to_lowercase())
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())),
            SortField::Size => a.size_bytes.cmp(&b.size_bytes)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())),
        };

        match order {
            SortOrder::Ascending => ordering,
            SortOrder::Descending => ordering.reverse(),
        }
    });
}

pub fn get_file_details(item_path: &Path) -> FileDetails {
    let Ok(meta) = fs::metadata(item_path) else {
        return FileDetails {
            name: item_path.file_name().unwrap_or_default().to_string_lossy().as_ref().into(),
            is_dir: false,
            file_type: "Unknown".into(),
            location: item_path.parent().unwrap_or(item_path).to_string_lossy().as_ref().into(),
            date_modified: "-".into(),
            size: "-".into(),
            has_selection: true,
        };
    };

    let is_dir = meta.is_dir();
    let name = item_path.file_name().unwrap_or_default().to_string_lossy().into_owned();
    let location = item_path.parent().unwrap_or(item_path).to_string_lossy().into_owned();
    let (date_str, _) = meta
        .modified()
        .ok()
        .map(format_system_time)
        .unwrap_or_else(|| ("-".to_string(), 0));

    let file_type = detect_file_type(item_path, is_dir);
    let size_str = if is_dir {
        "File folder".to_string()
    } else {
        format_size(meta.len())
    };

    FileDetails {
        name: name.into(),
        is_dir,
        file_type: file_type.into(),
        location: location.into(),
        date_modified: date_str.into(),
        size: size_str.into(),
        has_selection: true,
    }
}
