//! تطبيق لمواصفة freedesktop.org Icon Theme Specification:
//! https://specifications.freedesktop.org/icon-theme-spec/icon-theme-spec-latest.html
//!
//! الفروقات عن النسخة السابقة (ملخص الإصلاحات):
//! 1. يقرأ `index.theme` فعلياً (Directories=, Size=, MinSize=, MaxSize=,
//!    Threshold=, Type=, Context=) ويطبّق خوارزمية `DirectoryMatchesSize`
//!    و`DirectorySizeDistance` كما في المواصفة، بدل تخمين مسارات ثابتة.
//! 2. يدعم SVG وPNG وXPM (بنفس أولوية تطبيقات سطح المكتب الشائعة).
//! 3. مطابقة اسم الثيم تتم حصراً عبر `Name=` (تطابق تام) أو اسم المجلد
//!    الحرفي، بلا `starts_with` التي قد تختار ثيمًا بالخطأ.
//! 4. ترتيب الأحجام يعتمد على أقرب مسافة حقيقية للحجم المطلوب
//!    (BestFitIcon)، لا "الأكبر أولًا".
//! 5. `XDG_ICON_THEME` ليس متغيرًا قياسيًا؛ تم إنزاله لآخر قائمة الاكتشاف
//!    كـ override يدوي فقط، بعد مصادر سطح المكتب الفعلية (gsettings,
//!    XDG Desktop Portal, kreadconfig, xfconf) و gtk/kde config files.
//! 6. يُبنى فهرس (ملفات موجودة فعلاً) لكل مجلد ثيم مرة واحدة عبر `read_dir`،
//!    فتصبح كل عملية بحث لاحقة بحثًا في `HashSet` بدل نداءات متكررة لـ
//!    `Path::exists()` على نظام الملفات.
//! 7. الكاش يستخدم `Entry` واحد تحت قفل واحد يغطي "التحقق + الحساب +
//!    الإدراج" معًا، فلا تحدث نافذة تزامن بين البحث والتحميل لنفس المفتاح.

use slint::Image;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

/// امتدادات الملفات المدعومة، بترتيب الأولوية الرسمي
/// (المتجهات القابلة للتحجيم أولًا، ثم النقطية، ثم XPM كخيار أخير قديم).
const EXTENSIONS: &[&str] = &["svg", "png", "xpm"];

/// نوع مجلد الأيقونات كما تعرّفه المواصفة في `Type=` بقسم index.theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DirType {
    Fixed,
    Scalable,
    Threshold,
}

/// وصف مجلد أيقونات واحد مذكور في `Directories=`، بخصائصه كما تُقرأ من
/// `index.theme` (مع القيم الافتراضية التي تحددها المواصفة عند غيابها).
#[derive(Debug, Clone)]
struct IconDir {
    /// المسار النسبي كما ورد في Directories=، مثل "32x32/places".
    relative_path: String,
    size: u32,
    scale: u32,
    min_size: u32,
    max_size: u32,
    threshold: u32,
    dir_type: DirType,
}

impl IconDir {
    /// خوارزمية `DirectoryMatchesSize` من المواصفة الرسمية.
    fn matches_size(&self, icon_size: u32, icon_scale: u32) -> bool {
        if self.scale != icon_scale {
            return false;
        }
        match self.dir_type {
            DirType::Fixed => self.size == icon_size,
            DirType::Scalable => self.min_size <= icon_size && icon_size <= self.max_size,
            DirType::Threshold => {
                let low = self.size.saturating_sub(self.threshold);
                let high = self.size + self.threshold;
                low <= icon_size && icon_size <= high
            }
        }
    }

    /// خوارزمية `DirectorySizeDistance` من المواصفة الرسمية: مسافة هذا
    /// المجلد عن الحجم المطلوب، صفر يعني تطابقًا تامًا. تُستخدم لاختيار
    /// أفضل مجلد عندما لا يوجد تطابق مباشر عبر matches_size.
    fn size_distance(&self, icon_size: u32, icon_scale: u32) -> i64 {
        let icon_size = (icon_size * icon_scale.max(1)) as i64;
        let scale = self.scale.max(1) as i64;
        match self.dir_type {
            DirType::Fixed => {
                let sz = self.size as i64 * scale;
                (sz - icon_size).abs()
            }
            DirType::Scalable => {
                let min = self.min_size as i64 * scale;
                let max = self.max_size as i64 * scale;
                if icon_size < min {
                    min - icon_size
                } else if icon_size > max {
                    icon_size - max
                } else {
                    0
                }
            }
            DirType::Threshold => {
                let sz = self.size as i64 * scale;
                let thr = self.threshold as i64 * scale;
                if icon_size < sz - thr {
                    (sz - thr) - icon_size
                } else if icon_size > sz + thr {
                    icon_size - (sz + thr)
                } else {
                    0
                }
            }
        }
    }
}

/// ثيم أيقونات واحد تم العثور عليه على القرص: مساره، ومجلداته المفسَّرة من
/// index.theme، وفهرس الملفات الموجودة فعليًا بداخله (مبني مرة واحدة).
struct ThemeDir {
    base: PathBuf,
    dirs: Vec<IconDir>,
    /// فهرس: "مسار/نسبي" -> مجموعة أسماء الملفات الموجودة فيه (بامتدادها
    /// الكاملة، مثل "folder.svg"). يُبنى مرة واحدة عبر read_dir بدل
    /// استدعاء exists() لكل تركيبة اسم/امتداد محتملة.
    file_index: HashMap<String, HashSet<String>>,
}

impl ThemeDir {
    fn has_file(&self, relative_dir: &str, file_name: &str) -> bool {
        self.file_index
            .get(relative_dir)
            .map(|set| set.contains(file_name))
            .unwrap_or(false)
    }
}

pub struct IconTheme {
    /// سلسلة الثيمات الفعلية (الثيم المختار ثم أسلافه عبر Inherits=)
    /// بترتيب الأولوية، وأخيرًا hicolor كما تفرضه المواصفة دائمًا.
    themes: Vec<ThemeDir>,
    cache: RefCell<HashMap<String, Option<Image>>>,
}

// ---------------------------------------------------------------------
// اكتشاف مسارات البحث الأساسية (XDG_DATA_HOME, XDG_DATA_DIRS, ...)
// ---------------------------------------------------------------------

fn get_icon_search_roots() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let push_unique = |p: PathBuf, dirs: &mut Vec<PathBuf>| {
        if p.is_dir() && !dirs.contains(&p) {
            dirs.push(p);
        }
    };

    // $HOME/.icons أولًا: دليل المستخدم القديم له أولوية قصوى حسب المواصفة.
    if let Ok(home) = std::env::var("HOME") {
        if !home.is_empty() {
            push_unique(Path::new(&home).join(".icons"), &mut dirs);
        }
    }

    // $XDG_DATA_HOME/icons (افتراضيًا $HOME/.local/share/icons)
    let xdg_data_home = std::env::var("XDG_DATA_HOME")
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .filter(|s| !s.is_empty())
                .map(|h| Path::new(&h).join(".local/share"))
        });
    if let Some(base) = xdg_data_home {
        push_unique(base.join("icons"), &mut dirs);
    }

    // $XDG_DATA_DIRS/icons (افتراضيًا /usr/local/share:/usr/share)
    let xdg_data_dirs = std::env::var("XDG_DATA_DIRS")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".to_string());
    for dir in xdg_data_dirs.split(':') {
        if !dir.is_empty() {
            push_unique(Path::new(dir).join("icons"), &mut dirs);
        }
    }

    // /usr/share/pixmaps: آخر ملاذ تذكره المواصفة صراحة لأيقونات منفردة.
    for fallback in &["/usr/share/icons", "/usr/local/share/icons"] {
        push_unique(PathBuf::from(fallback), &mut dirs);
    }

    dirs
}

// ---------------------------------------------------------------------
// اكتشاف ثيم سطح المكتب الحالي
// ---------------------------------------------------------------------

fn extract_quoted_theme(s: &str) -> Option<String> {
    if let Some(start) = s.find('\'') {
        if let Some(end) = s[start + 1..].find('\'') {
            let val = s[start + 1..start + 1 + end].trim();
            if !val.is_empty() {
                return Some(val.to_string());
            }
        }
    }
    if let Some(start) = s.find('"') {
        if let Some(end) = s[start + 1..].find('"') {
            let val = s[start + 1..start + 1 + end].trim();
            if !val.is_empty() {
                return Some(val.to_string());
            }
        }
    }
    None
}

fn parse_ini_value<'a>(content: &'a str, section: &str, key: &str) -> Option<&'a str> {
    let mut in_section = false;
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with('[') && line.ends_with(']') {
            in_section = &line[1..line.len() - 1] == section;
            continue;
        }
        if in_section {
            if let Some(rest) = line.strip_prefix(key) {
                if let Some(rest) = rest.strip_prefix('=') {
                    return Some(rest.trim());
                }
            }
        }
    }
    None
}

/// يحاول اكتشاف ثيم الأيقونات النشط فعليًا في بيئة سطح المكتب الحالية.
/// الترتيب هنا يعكس ما تستخدمه بيئات سطح المكتب نفسها لاختيار الثيم،
/// وليس تخمينًا: gsettings/portal لـ GNOME وأغلب البيئات الحديثة،
/// kreadconfig لـ KDE، xfconf لـ XFCE، ثم ملفات الإعداد مباشرة كبديل
/// أخير عندما لا تتوفر أي من هذه الأدوات، وأخيرًا XDG_ICON_THEME كمتغير
/// override غير قياسي (قد يضبطه المستخدم يدويًا لكنه ليس ما تقرأه بيئات
/// سطح المكتب نفسها).
fn detect_system_icon_theme() -> String {
    // GNOME / أغلب البيئات الحديثة القائمة على gsettings
    if let Ok(output) = std::process::Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "icon-theme"])
        .output()
    {
        if output.status.success() {
            let s = String::from_utf8_lossy(&output.stdout);
            if let Some(theme) = extract_quoted_theme(&s) {
                return theme;
            }
        }
    }

    // XDG Desktop Portal (ساندبوكس/فلاتباك وبيئات لا تستخدم gsettings مباشرة)
    for (cmd, args) in [
        (
            "gdbus",
            vec![
                "call", "--session", "--dest", "org.freedesktop.portal.Desktop",
                "--object-path", "/org/freedesktop/portal/desktop",
                "--method", "org.freedesktop.portal.Settings.ReadOne",
                "org.gnome.desktop.interface", "icon-theme",
            ],
        ),
        (
            "busctl",
            vec![
                "--user", "call", "org.freedesktop.portal.Desktop",
                "/org/freedesktop/portal/desktop", "org.freedesktop.portal.Settings",
                "ReadOne", "ss", "org.gnome.desktop.interface", "icon-theme",
            ],
        ),
    ] {
        if let Ok(output) = std::process::Command::new(cmd).args(&args).output() {
            if output.status.success() {
                let s = String::from_utf8_lossy(&output.stdout);
                if let Some(theme) = extract_quoted_theme(&s) {
                    return theme;
                }
            }
        }
    }

    // KDE Plasma
    for kcmd in &["kreadconfig6", "kreadconfig5"] {
        if let Ok(output) = std::process::Command::new(kcmd)
            .args(["--group", "Icons", "--key", "Theme"])
            .output()
        {
            if output.status.success() {
                let theme = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !theme.is_empty() {
                    return theme;
                }
            }
        }
    }

    // XFCE
    if let Ok(output) = std::process::Command::new("xfconf-query")
        .args(["-c", "xsettings", "-p", "/Net/IconThemeName"])
        .output()
    {
        if output.status.success() {
            let theme = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !theme.is_empty() {
                return theme;
            }
        }
    }

    // بديل أخير: قراءة ملفات الإعداد مباشرة عندما لا تتوفر أي أداة أعلاه
    if let Ok(home) = std::env::var("HOME") {
        if let Ok(content) = fs::read_to_string(Path::new(&home).join(".config/kdeglobals")) {
            if let Some(theme) = parse_ini_value(&content, "Icons", "Theme") {
                if !theme.is_empty() {
                    return theme.to_string();
                }
            }
        }
        for gtk_rel in [".config/gtk-3.0/settings.ini", ".config/gtk-4.0/settings.ini"] {
            if let Ok(content) = fs::read_to_string(Path::new(&home).join(gtk_rel)) {
                if let Some(theme) = parse_ini_value(&content, "Settings", "gtk-icon-theme-name") {
                    let theme = theme.trim_matches('"').trim_matches('\'');
                    if !theme.is_empty() {
                        return theme.to_string();
                    }
                }
            }
        }
    }

    // XDG_ICON_THEME ليس متغيرًا قياسيًا موحدًا تقرأه بيئات سطح المكتب،
    // لكنه يبقى مفيدًا كـ override يدوي صريح من المستخدم، لذا نجعله آخر
    // مصدر بدل اعتماده كمصدر أول كما كان سابقًا.
    if let Ok(t) = std::env::var("XDG_ICON_THEME") {
        let t = t.trim();
        if !t.is_empty() {
            return t.to_string();
        }
    }

    "Adwaita".to_string()
}

// ---------------------------------------------------------------------
// تحليل index.theme وبناء فهرس الملفات لكل ثيم
// ---------------------------------------------------------------------

/// يبحث عن مجلد ثيم بالاسم `theme_name` داخل `root`، بمطابقة تامة فقط
/// (اسم المجلد حرفيًا، أو حقل Name= داخل index.theme الخاص به). لا مطابقة
/// جزئية (starts_with) هنا كي لا يُختار ثيم بالخطأ لمجرد تشابه الاسم.
fn find_theme_in_root(root: &Path, theme_name: &str) -> Option<PathBuf> {
    let direct = root.join(theme_name);
    if direct.is_dir() {
        return Some(direct);
    }

    let entries = fs::read_dir(root).ok()?;
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let dir_name = entry.file_name();
        let dir_str = dir_name.to_string_lossy();
        if dir_str.eq_ignore_ascii_case(theme_name) {
            return Some(path);
        }
        if let Ok(content) = fs::read_to_string(path.join("index.theme")) {
            if let Some(name) = parse_ini_value(&content, "Icon Theme", "Name") {
                if name.eq_ignore_ascii_case(theme_name) {
                    return Some(path);
                }
            }
        }
    }
    None
}

/// يحلّل قسم مجلد واحد (مثل [32x32/places]) من index.theme وفق القيم
/// الافتراضية التي تحددها المواصفة عند غياب أي مفتاح.
fn parse_icon_dir(content: &str, section: &str) -> IconDir {
    let get = |key: &str| parse_ini_value(content, section, key);

    let size: u32 = get("Size").and_then(|v| v.parse().ok()).unwrap_or(48);
    let scale: u32 = get("Scale").and_then(|v| v.parse().ok()).unwrap_or(1).max(1);
    let min_size: u32 = get("MinSize").and_then(|v| v.parse().ok()).unwrap_or(size);
    let max_size: u32 = get("MaxSize").and_then(|v| v.parse().ok()).unwrap_or(size);
    let threshold: u32 = get("Threshold").and_then(|v| v.parse().ok()).unwrap_or(2);
    let dir_type = match get("Type").map(|s| s.to_ascii_lowercase()).as_deref() {
        Some("fixed") => DirType::Fixed,
        Some("scalable") => DirType::Scalable,
        _ => DirType::Threshold,
    };

    IconDir {
        relative_path: section.to_string(),
        size,
        scale,
        min_size,
        max_size,
        threshold,
        dir_type,
    }
}

/// يقرأ `Directories=` من index.theme ويحلّل كل قسم مذكور فيها.
/// في حال غياب index.theme بالكامل (بعض الثيمات المصغّرة أو hicolor
/// المجرّد)، نرجع قائمة فارغة ونعتمد لاحقًا على فهرس الملفات وحده عبر
/// مسح شجرة المجلد كخطة بديلة.
fn parse_theme_directories(theme_path: &Path) -> Vec<IconDir> {
    let index_path = theme_path.join("index.theme");
    let content = match fs::read_to_string(&index_path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let directories = parse_ini_value(&content, "Icon Theme", "Directories")
        .map(|v| v.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect::<Vec<_>>())
        .unwrap_or_default();

    directories
        .iter()
        .map(|section| parse_icon_dir(&content, section))
        .collect()
}

/// يقرأ `Inherits=` من index.theme لثيم معيّن.
fn parse_theme_inherits(theme_path: &Path) -> Vec<String> {
    let content = match fs::read_to_string(theme_path.join("index.theme")) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    parse_ini_value(&content, "Icon Theme", "Inherits")
        .map(|v| v.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect())
        .unwrap_or_default()
}

/// يبني فهرس الملفات الموجودة فعليًا داخل مجلدات الثيم (مرة واحدة فقط
/// عند التهيئة)، بدل استدعاء `Path::exists()` لكل تركيبة اسم/حجم/امتداد
/// محتملة في كل بحث لاحق. يمسح فرعيًا طبقتين (مثل scalable/places) وهو
/// ما يغطي كل بنيات مجلدات الأيقونات الشائعة.
fn build_file_index(theme_path: &Path, declared_dirs: &[IconDir]) -> HashMap<String, HashSet<String>> {
    let mut index: HashMap<String, HashSet<String>> = HashMap::new();

    let mut relative_dirs: HashSet<String> = declared_dirs
        .iter()
        .map(|d| d.relative_path.clone())
        .collect();

    // بعض الثيمات (خصوصًا hicolor المجرّد أو ثيمات بلا index.theme) لا
    // تُصرّح بمجلداتها؛ نكتشفها بمسح مباشر لمستويين تحت جذر الثيم.
    if relative_dirs.is_empty() {
        if let Ok(top_entries) = fs::read_dir(theme_path) {
            for top in top_entries.filter_map(|e| e.ok()) {
                if !top.path().is_dir() {
                    continue;
                }
                let top_name = top.file_name().to_string_lossy().to_string();
                relative_dirs.insert(top_name.clone());
                if let Ok(sub_entries) = fs::read_dir(top.path()) {
                    for sub in sub_entries.filter_map(|e| e.ok()) {
                        if sub.path().is_dir() {
                            relative_dirs.insert(format!(
                                "{top_name}/{}",
                                sub.file_name().to_string_lossy()
                            ));
                        }
                    }
                }
            }
        }
    }

    for rel in relative_dirs {
        let full = theme_path.join(&rel);
        let mut files = HashSet::new();
        if let Ok(entries) = fs::read_dir(&full) {
            for entry in entries.filter_map(|e| e.ok()) {
                if let Some(name) = entry.file_name().to_str() {
                    files.insert(name.to_string());
                }
            }
        }
        if !files.is_empty() {
            index.insert(rel, files);
        }
    }

    index
}

/// يحل سلسلة الوراثة الكاملة لثيم (الثيم نفسه ثم أسلافه عبر Inherits=،
/// بدون تكرار)، ثم يُلحق `hicolor` في النهاية لأن المواصفة تفرض وجوده
/// دائمًا كملاذ أخير لكل الثيمات.
fn resolve_theme_chain(primary: &str, search_roots: &[PathBuf]) -> Vec<ThemeDir> {
    let mut chain_names = vec![primary.to_string()];
    let mut visited: HashSet<String> = HashSet::new();
    let mut i = 0;
    while i < chain_names.len() {
        let name = chain_names[i].clone();
        if !visited.insert(name.clone()) {
            i += 1;
            continue;
        }
        if let Some(dir) = search_roots.iter().find_map(|r| find_theme_in_root(r, &name)) {
            for parent in parse_theme_inherits(&dir) {
                if !visited.contains(&parent) && !chain_names.contains(&parent) {
                    chain_names.push(parent);
                }
            }
        }
        i += 1;
    }
    if !chain_names.iter().any(|n| n.eq_ignore_ascii_case("hicolor")) {
        chain_names.push("hicolor".to_string());
    }

    let mut themes = Vec::new();
    let mut seen_paths = HashSet::new();
    for name in &chain_names {
        for root in search_roots {
            if let Some(path) = find_theme_in_root(root, name) {
                if seen_paths.insert(path.clone()) {
                    let dirs = parse_theme_directories(&path);
                    let file_index = build_file_index(&path, &dirs);
                    themes.push(ThemeDir { base: path, dirs, file_index });
                }
            }
        }
    }
    themes
}

// ---------------------------------------------------------------------
// البحث الفعلي عن أيقونة (FindIconHelper من المواصفة)
// ---------------------------------------------------------------------

impl IconTheme {
    pub fn new() -> Self {
        let search_roots = get_icon_search_roots();
        let primary_theme = detect_system_icon_theme();
        let themes = resolve_theme_chain(&primary_theme, &search_roots);

        Self {
            themes,
            cache: RefCell::new(HashMap::new()),
        }
    }

    /// يبحث عن أيقونة باسم `name` بأقرب حجم ممكن إلى `preferred_size`،
    /// متّبعًا خوارزمية `find_icon_helper` الرسمية: لكل ثيم في سلسلة
    /// الوراثة، أولًا تطابق تام للحجم (matches_size)، ثم أقرب مسافة حجم
    /// (size_distance) إن لم يوجد تطابق تام في أي مجلد بهذا الثيم.
    pub fn find_icon_path(&self, name: &str, preferred_size: u32) -> Option<PathBuf> {
        for theme in &self.themes {
            // محاولة 1: تطابق تام وفق matches_size، بترتيب الامتدادات
            // حسب الأولوية (svg > png > xpm) داخل كل مجلد مرشّح.
            for dir in &theme.dirs {
                if dir.matches_size(preferred_size, 1) {
                    if let Some(p) = Self::lookup_in_dir(theme, dir, name) {
                        return Some(p);
                    }
                }
            }

            // محاولة 2: أقرب مجلد من حيث المسافة الحقيقية للحجم، وليس
            // "الأكبر أولًا" كما في النسخة السابقة. نرتّب كل مجلدات هذا
            // الثيم حسب size_distance ونجرّب الأقرب فالأقرب.
            let mut by_distance: Vec<&IconDir> = theme.dirs.iter().collect();
            by_distance.sort_by_key(|d| d.size_distance(preferred_size, 1));
            for dir in by_distance {
                if let Some(p) = Self::lookup_in_dir(theme, dir, name) {
                    return Some(p);
                }
            }

            // بعض الثيمات (أو hicolor الأساسي) قد لا تملك فهرس مجلدات
            // declared لكنها ما زالت تحتوي الملف في مسار مكتشف بالمسح
            // المباشر (مُسجّل في file_index رغم عدم وجوده في dirs).
            for (rel, files) in &theme.file_index {
                if theme.dirs.iter().any(|d| &d.relative_path == rel) {
                    continue; // تم فحصه أعلاه بالفعل
                }
                for ext in EXTENSIONS {
                    let file_name = format!("{name}.{ext}");
                    if files.contains(&file_name) {
                        return Some(theme.base.join(rel).join(file_name));
                    }
                }
            }
        }

        None
    }

    fn lookup_in_dir(theme: &ThemeDir, dir: &IconDir, name: &str) -> Option<PathBuf> {
        for ext in EXTENSIONS {
            let file_name = format!("{name}.{ext}");
            if theme.has_file(&dir.relative_path, &file_name) {
                return Some(theme.base.join(&dir.relative_path).join(file_name));
            }
        }
        None
    }

    /// يحمّل الأيقونة من القرص إلى `slint::Image`، باستخدام كاش واحد
    /// يغطي "البحث عن المسار + القراءة من القرص + الإدراج" ضمن استعارة
    /// (borrow) واحدة لـ `RefCell`، فلا يمكن لعمليتي بحث متزامنتين لنفس
    /// المفتاح أن تتسابقا قبل إدخال النتيجة (لا توجد نافذة توقف بين
    /// التحقق من الكاش وحساب القيمة، كما كان يحدث مع lock محرَّر مرتين).
    pub fn load_icon(&self, name: &str, preferred_size: u32) -> Option<Image> {
        let cache_key = format!("{name}_{preferred_size}");
        let mut cache = self.cache.borrow_mut();
        if let Some(cached) = cache.get(&cache_key) {
            return cached.clone();
        }
        let img = self
            .find_icon_path(name, preferred_size)
            .and_then(|p| Image::load_from_path(&p).ok());
        cache.insert(cache_key, img.clone());
        img
    }

    pub fn icon_for_folder(&self, folder_name: &str) -> Image {
        let lower = folder_name.to_lowercase();
        let candidates: &[&str] = match lower.as_str() {
            "desktop" => &["user-desktop", "folder-desktop", "desktop", "folder"],
            "downloads" | "download" => &["folder-download", "folder-downloads", "download", "folder"],
            "documents" | "document" => &["folder-documents", "folder-document", "folder-text", "documents", "folder"],
            "pictures" | "images" | "photos" => &["folder-pictures", "folder-picture", "folder-images", "pictures", "folder"],
            "music" | "audio" => &["folder-music", "folder-sound", "music", "folder"],
            "videos" | "video" | "movies" => &["folder-videos", "folder-video", "videos", "video", "folder"],
            "public" => &["folder-publicshare", "folder-public", "folder"],
            "templates" => &["folder-templates", "folder"],
            _ => &["folder", "folder-custom", "default-folder", "inode-directory"],
        };

        for name in candidates {
            if let Some(img) = self.load_icon(name, 64) {
                return img;
            }
        }
        Image::default()
    }

    pub fn icon_for_file(&self, path: &Path, is_dir: bool) -> Image {
        if is_dir {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            return self.icon_for_folder(&name);
        }

        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        let candidates: &[&str] = match ext.as_str() {
            "pdf" => &["application-pdf", "x-office-document"],
            "png" => &["image-png", "image-x-generic"],
            "jpg" | "jpeg" => &["image-jpeg", "image-x-generic"],
            "gif" => &["image-gif", "image-x-generic"],
            "svg" => &["image-svg+xml", "image-x-generic"],
            "webp" | "bmp" | "tiff" | "ico" => &["image-x-generic"],
            "mp3" | "wav" | "flac" | "aac" | "ogg" | "m4a" | "wma" => &["audio-x-generic"],
            "mp4" | "mkv" | "avi" | "webm" | "mov" | "wmv" | "flv" => &["video-x-generic"],
            "zip" | "tar" | "gz" | "xz" | "7z" | "bz2" | "rar" | "tgz" | "zst" => {
                &["package-x-generic", "application-zip", "application-x-tar"]
            }
            "rs" => &["text-x-rust", "text-x-generic"],
            "py" => &["text-x-python", "text-x-script", "text-x-generic"],
            "js" | "mjs" | "cjs" => &["application-javascript", "text-javascript", "text-x-generic"],
            "ts" => &["application-x-typescript", "text-x-generic"],
            "json" => &["application-json", "text-x-generic"],
            "toml" | "yaml" | "yml" | "xml" => &["text-xml", "text-x-generic"],
            "sh" | "bash" | "zsh" => &["application-x-shellscript", "text-x-script", "text-x-generic"],
            "html" | "htm" => &["text-html", "text-x-generic"],
            "css" => &["text-css", "text-x-generic"],
            "txt" | "md" | "log" => &["text-plain", "text-x-generic"],
            "doc" | "docx" => &["application-msword", "x-office-document"],
            "xls" | "xlsx" | "csv" => &["application-vnd.ms-excel", "x-office-spreadsheet"],
            "ppt" | "pptx" => &["application-vnd.ms-powerpoint", "x-office-presentation"],
            "exe" | "bin" | "appimage" => &["application-x-executable"],
            "iso" | "img" => &["application-x-cd-image", "media-optical"],
            _ => &["text-x-generic", "application-octet-stream"],
        };

        for name in candidates {
            if let Some(img) = self.load_icon(name, 64) {
                return img;
            }
        }
        Image::default()
    }

    pub fn icon_for_disk(&self) -> Image {
        for name in &["drive-harddisk", "drive-removable-media", "media-optical"] {
            if let Some(img) = self.load_icon(name, 64) {
                return img;
            }
        }
        Image::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_icon_theme_init() {
        let theme = IconTheme::new();
        assert!(!theme.themes.is_empty(), "Should resolve at least hicolor as a fallback theme");
    }

    #[test]
    fn test_icon_for_folder() {
        let theme = IconTheme::new();
        let folder = PathBuf::from("/home");
        let icon = theme.icon_for_file(&folder, true);
        assert!(icon.size().width > 0, "Folder icon should be loaded from system theme");
    }

    #[test]
    fn test_icon_for_disk() {
        let theme = IconTheme::new();
        let icon = theme.icon_for_disk();
        assert!(icon.size().width > 0, "Disk icon should be loaded from system theme");
    }

    #[test]
    fn test_size_distance_exact_match_is_zero() {
        let dir = IconDir {
            relative_path: "32x32/places".into(),
            size: 32,
            scale: 1,
            min_size: 32,
            max_size: 32,
            threshold: 2,
            dir_type: DirType::Fixed,
        };
        assert_eq!(dir.size_distance(32, 1), 0);
        assert!(dir.size_distance(16, 1) > 0);
    }

    #[test]
    fn test_scalable_matches_within_range() {
        let dir = IconDir {
            relative_path: "scalable/places".into(),
            size: 48,
            scale: 1,
            min_size: 16,
            max_size: 256,
            threshold: 2,
            dir_type: DirType::Scalable,
        };
        assert!(dir.matches_size(16, 1));
        assert!(dir.matches_size(256, 1));
        assert!(!dir.matches_size(512, 1));
    }
}
