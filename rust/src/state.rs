use crate::{model::*, services::Service};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock},
    time::{Duration, Instant},
};
pub const PAGES: [&str; 8] = [
    "Processes",
    "Performance",
    "App history",
    "Startup apps",
    "Users",
    "Details",
    "Services",
    "Settings",
];
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub start: usize,
    pub speed: u64,
    pub dark: bool,
    pub system_theme: bool,
    pub top: bool,
    pub minimize: bool,
    pub hide: bool,
    pub grouped: bool,
    pub binary: bool,
    pub logical: bool,
    pub kernel: bool,
    pub graph_seconds: usize,
    pub restore: HashMap<String, u8>,
    pub hidden: HashMap<usize, HashSet<String>>,
    pub history_all: bool,
    pub widths: HashMap<String, f64>,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            start: 0,
            speed: 2,
            dark: false,
            system_theme: false,
            top: false,
            minimize: false,
            hide: false,
            grouped: true,
            binary: false,
            logical: false,
            kernel: false,
            graph_seconds: 60,
            restore: HashMap::new(),
            hidden: HashMap::new(),
            history_all: false,
            widths: HashMap::new(),
        }
    }
}
pub fn support() -> std::path::PathBuf {
    std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default())
        .join("Library/Application Support/TaskManager")
}
impl Preferences {
    pub fn load() -> Self {
        std::fs::read(support().join("rust-preferences.json"))
            .ok()
            .and_then(|d| serde_json::from_slice(&d).ok())
            .unwrap_or_default()
    }
    pub fn save(&self) {
        let _ = std::fs::create_dir_all(support());
        if let Ok(s) = serde_json::to_vec_pretty(self) {
            let p = support().join("rust-preferences.json");
            let tmp = p.with_extension("tmp");
            if std::fs::write(&tmp, s).is_ok() {
                let _ = std::fs::rename(tmp, p);
            }
        }
    }
}
pub fn refresh_interval(speed: u64) -> Option<Duration> {
    match speed {
        0 => None,
        1 => Some(Duration::from_millis(250)),
        4 => Some(Duration::from_secs(4)),
        _ => Some(Duration::from_millis(500)),
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ServiceRequest {
    pub action: String,
    pub items: Vec<Service>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    None,
    Page(usize),
    Resource(usize),
    Collapse,
    Minimize,
    Maximize,
    Close,
    Sort(String),
    Select(String),
    Expand(String),
    Run,
    End,
    Efficiency,
    View,
    Menu(String),
    Option(String),
    Copy,
    Properties,
    Reveal,
    SearchOnline,
    GoDetails,
    Switch,
    Signal(i32),
    Priority(i32),
    Service(String),
    ConfirmServices(Box<ServiceRequest>),
    Refresh,
    ResetHistory,
    GraphOnly,
    Summary,
    Kernel,
    Logical(bool),
    Snap(u8),
    Setting(String),
    Quit,
    About,
    ScrollV,
    ScrollH,
    ResizeColumn(usize),
    DialogOk,
    DialogCancel,
}
#[derive(Clone)]
pub struct Hit {
    pub rect: [f64; 4],
    pub action: Action,
    pub label: String,
    pub enabled: bool,
}
impl Hit {
    pub fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.rect[0]
            && x < self.rect[0] + self.rect[2]
            && y >= self.rect[1]
            && y < self.rect[1] + self.rect[3]
    }
}
#[derive(Clone)]
pub struct MenuItem {
    pub title: String,
    pub action: Action,
    pub enabled: bool,
    pub checked: bool,
    pub children: Vec<MenuItem>,
}
impl MenuItem {
    pub fn new(s: &str, a: Action) -> Self {
        Self {
            title: s.into(),
            action: a,
            enabled: true,
            checked: false,
            children: vec![],
        }
    }
    pub fn disabled(s: &str) -> Self {
        Self {
            enabled: false,
            ..Self::new(s, Action::None)
        }
    }
    pub fn sep() -> Self {
        Self::new("", Action::None)
    }
    pub fn sub(s: &str, children: Vec<MenuItem>) -> Self {
        Self {
            children,
            ..Self::new(s, Action::None)
        }
    }
    pub fn check(s: &str, a: Action, b: bool) -> Self {
        Self {
            checked: b,
            ..Self::new(s, a)
        }
    }
}
pub struct Menu {
    pub x: f64,
    pub y: f64,
    pub items: Vec<MenuItem>,
    pub selected: Option<usize>,
}
#[derive(Clone)]
pub struct Column {
    pub key: String,
    pub title: String,
    pub width: f64,
    pub numeric: bool,
}
fn col(k: &str, t: &str, w: f64, n: bool) -> Column {
    Column {
        key: k.into(),
        title: t.into(),
        width: w,
        numeric: n,
    }
}
pub fn columns(page: usize) -> Vec<Column> {
    match page {
        0 => vec![
            col("name", "Name", 290., false),
            col("status", "Status", 130., false),
            col("cpu", "CPU", 85., true),
            col("memory", "Memory", 100., true),
            col("disk", "Disk", 90., true),
            col("network", "Network", 90., true),
            col("gpu", "GPU", 80., true),
            col("pid", "PID", 70., true),
        ],
        2 => vec![
            col("name", "Name", 280., false),
            col("time", "CPU time", 110., true),
            col("network", "Network", 110., true),
            col("metered", "Metered network", 140., true),
            col("tile", "Notifications", 120., true),
        ],
        3 => vec![
            col("name", "Name", 290., false),
            col("publisher", "Publisher", 180., false),
            col("status", "Status", 110., false),
            col("kind", "Startup type", 160., false),
            col("impact", "Startup impact", 140., false),
        ],
        4 => vec![
            col("name", "User", 260., false),
            col("status", "Status", 130., false),
            col("cpu", "CPU", 90., true),
            col("memory", "Memory", 110., true),
            col("disk", "Disk", 90., true),
            col("network", "Network", 90., true),
        ],
        5 => vec![
            col("name", "Name", 260., false),
            col("pid", "PID", 75., true),
            col("status", "Status", 115., false),
            col("user", "User name", 155., false),
            col("cpu", "CPU", 85., true),
            col("memory", "Memory", 110., true),
            col("arch", "Architecture", 100., false),
            col("description", "Description", 250., false),
            col("threads", "Threads", 85., true),
            col("time", "CPU time", 100., true),
            col("path", "Image path name", 320., false),
            col("nice", "Base priority", 100., true),
            col("ppid", "Parent PID", 90., true),
            col("bundle", "Bundle identifier", 220., false),
            col("start", "Start time (Unix)", 150., true),
        ],
        6 => vec![
            col("name", "Name", 340., false),
            col("pid", "PID", 75., true),
            col("description", "Description", 300., false),
            col("status", "Status", 110., false),
            col("group", "Group", 100., false),
        ],
        _ => vec![],
    }
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RowMetrics {
    pub cpu: Option<f64>,
    pub memory: Option<f64>,
    pub disk: Option<f64>,
    pub threads: Option<f64>,
    pub time: Option<f64>,
    pub pid: Option<f64>,
}

impl RowMetrics {
    fn for_processes(records: &[Process], members: &[usize]) -> Self {
        let mut valid = false;
        let mut io_valid = false;
        let mut threads_known = false;
        let (mut cpu, mut memory, mut disk, mut threads, mut time) = (0., 0., 0., 0., 0.);
        for &i in members {
            let process = &records[i];
            if process.raw.valid != 0 {
                valid = true;
                cpu += process.cpu;
                memory += process.raw.memory as f64;
                time += process.raw.cpu_ns as f64 / 1e9;
                if process.raw.threads >= 0 {
                    threads_known = true;
                    threads += f64::from(process.raw.threads);
                }
            }
            if process.raw.io_valid != 0 {
                io_valid = true;
                disk += process.read + process.write;
            }
        }
        Self {
            cpu: valid.then_some(cpu),
            memory: valid.then_some(memory),
            disk: io_valid.then_some(disk),
            threads: threads_known.then_some(threads),
            time: valid.then_some(time),
            pid: Some(f64::from(records[members[0]].raw.pid)),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Row {
    pub id: String,
    pub name: String,
    pub values: HashMap<String, String>,
    pub metrics: RowMetrics,
    pub members: Vec<usize>,
    pub service: Option<usize>,
    pub icon: String,
    pub group: bool,
    pub section: bool,
    pub indent: bool,
}
impl Row {
    fn section(s: &str) -> Self {
        Self {
            id: format!("section:{s}"),
            name: s.into(),
            section: true,
            ..Default::default()
        }
    }
}
pub fn vertical_scroll_thumb(viewport: f64, max: f64, offset: f64) -> Option<(f64, f64)> {
    if max <= 0. {
        return None;
    }
    let height = (viewport / (viewport + max) * viewport).max(28.);
    Some((offset / max * (viewport - height), height))
}
pub struct Dialog {
    pub title: String,
    pub message: String,
    pub ok: String,
    pub action: Action,
    pub input: bool,
}
pub enum Delivery {
    Operation(Result<String, String>),
    ServiceChange(Result<String, String>),
    Power(Result<(String, u8, u8), String>),
    Inspector(u64, String),
}
pub struct Shared {
    pub latest: Option<Sample>,
    pub services: Option<Vec<Service>>,
    pub results: VecDeque<Delivery>,
    pub speed: u64,
    pub refresh: bool,
    reset_sample: bool,
    pub scan: bool,
    pub services_visible: bool,
    startup_visible: bool,
    with_threads: bool,
    pub stop: bool,
}
pub struct Feed {
    pub data: Mutex<Shared>,
    pub wake: Condvar,
    pub on_delivery: OnceLock<Box<dyn Fn() + Send + Sync>>,
}

impl Feed {
    fn wait<'a>(
        &self,
        state: MutexGuard<'a, Shared>,
        until: Option<Instant>,
    ) -> MutexGuard<'a, Shared> {
        match until {
            Some(until) => {
                self.wake
                    .wait_timeout(state, until.saturating_duration_since(Instant::now()))
                    .unwrap()
                    .0
            }
            None => self.wake.wait(state).unwrap(),
        }
    }

    pub fn notify_ui(&self) {
        if let Some(notify) = self.on_delivery.get() {
            notify();
        }
    }

    pub fn deliver(&self, result: Delivery) {
        self.data.lock().unwrap().results.push_back(result);
        self.notify_ui();
    }

    pub fn set_speed(&self, speed: u64) {
        let mut state = self.data.lock().unwrap();
        if state.speed == speed {
            return;
        }
        if (state.speed == 0) != (speed == 0) {
            state.reset_sample = speed != 0;
            state.refresh = speed != 0;
            state.latest = None;
        }
        state.speed = speed;
        self.wake.notify_all();
    }

    pub fn stop(&self) {
        self.data.lock().unwrap().stop = true;
        self.wake.notify_all();
    }
}

pub struct State {
    pub prefs: Preferences,
    pub page: usize,
    pub resource: usize,
    pub collapsed: bool,
    pub records: Vec<Process>,
    pub system: Snapshot,
    pub services: Vec<Service>,
    pub history: HistoryStore,
    pub samples: VecDeque<GraphSample>,
    pub rows: Vec<Row>,
    pub columns: Vec<Column>,
    pub expanded: HashSet<String>,
    pub regular_apps: HashSet<String>,
    pub selected: Option<String>,
    pub selections: HashSet<String>,
    pub query: String,
    pub sort: String,
    pub ascending: bool,
    pub scroll: f64,
    pub hscroll: f64,
    pub hits: Vec<Hit>,
    pub hover: Action,
    pub focus: Action,
    pub menus: Vec<Menu>,
    pub dialog: Option<Dialog>,
    pub dialog_scroll: f64,
    pub inspector_generation: u64,
    pub graph_only: bool,
    pub summary: bool,
    pub memory_percent: bool,
    pub login_enabled: bool,
    pub busy: bool,
    pub service_change_pending: bool,
    pub shared: Arc<Feed>,
    pub width: f64,
    pub height: f64,
    pub settings_height: f64,
    pub drag: Option<(Action, f64, f64, f64)>,
    pub last_save: Instant,
    pub redraw_ms: VecDeque<f64>,
}
impl State {
    pub fn new() -> Self {
        let prefs = Preferences::load();
        let mut history = HistoryStore::default();
        history.entries = std::fs::read(support().join("rust-history.json"))
            .ok()
            .and_then(|s| serde_json::from_slice(&s).ok())
            .unwrap_or_default();
        let services_visible = matches!(prefs.start, 3 | 6);
        let shared = Arc::new(Feed {
            data: Mutex::new(Shared {
                latest: None,
                services: None,
                results: VecDeque::new(),
                speed: prefs.speed,
                refresh: true,
                reset_sample: false,
                scan: services_visible,
                services_visible,
                startup_visible: prefs.start == 3,
                with_threads: matches!(prefs.start, 1 | 5),
                stop: false,
            }),
            wake: Condvar::new(),
            on_delivery: OnceLock::new(),
        });
        let details = prefs.start == 5;
        let mut s = Self {
            page: prefs.start.min(7),
            prefs,
            resource: 0,
            collapsed: false,
            records: vec![],
            system: Snapshot::default(),
            services: vec![],
            history,
            samples: VecDeque::new(),
            rows: vec![],
            columns: vec![],
            expanded: HashSet::new(),
            regular_apps: HashSet::new(),
            selected: None,
            selections: HashSet::new(),
            query: String::new(),
            sort: if details { "cpu" } else { "name" }.into(),
            ascending: !details,
            scroll: 0.,
            hscroll: 0.,
            hits: vec![],
            hover: Action::None,
            focus: Action::None,
            menus: vec![],
            dialog: None,
            dialog_scroll: 0.,
            inspector_generation: 0,
            graph_only: false,
            summary: false,
            memory_percent: false,
            login_enabled: false,
            busy: false,
            service_change_pending: false,
            shared,
            width: 1120.,
            height: 720.,
            settings_height: 960.,
            drag: None,
            last_save: Instant::now(),
            redraw_ms: VecDeque::new(),
        };
        s.reset_columns();
        s
    }
    pub fn save(&self) {
        self.prefs.save();
        if let Ok(s) = serde_json::to_vec(&self.history.entries) {
            let _ = std::fs::write(support().join("rust-history.json"), s);
        }
    }
    pub fn sidebar(&self) -> f64 {
        if self.graph_only {
            0.
        } else if self.collapsed {
            48.
        } else {
            240.
        }
    }
    pub fn reset_columns(&mut self) {
        let hidden = self.prefs.hidden.get(&self.page);
        self.columns = columns(self.page)
            .into_iter()
            .filter(|c| {
                if let Some(hidden) = hidden {
                    !hidden.contains(&c.key)
                } else {
                    match self.page {
                        0 => !["gpu", "pid"].contains(&c.key.as_str()),
                        5 => !["threads", "time", "path", "nice", "ppid", "bundle", "start"]
                            .contains(&c.key.as_str()),
                        _ => true,
                    }
                }
            })
            .map(|mut c| {
                if let Some(w) = self.prefs.widths.get(&format!("{}:{}", self.page, c.key)) {
                    c.width = (*w).clamp(55., 900.);
                }
                c
            })
            .collect()
    }
    pub fn set_page(&mut self, n: usize) {
        self.page = n.min(7);
        {
            let mut shared = self.shared.data.lock().unwrap();
            let visible = matches!(self.page, 3 | 6);
            if visible && (!shared.services_visible || shared.startup_visible != (self.page == 3)) {
                shared.scan = true;
            }
            shared.services_visible = visible;
            shared.startup_visible = self.page == 3;
            let with_threads = matches!(self.page, 1 | 5);
            if with_threads && !shared.with_threads && shared.speed != 0 {
                shared.refresh = true;
                shared.latest = None;
            }
            shared.with_threads = with_threads;
            self.shared.wake.notify_all();
        }
        self.sort = if n == 5 { "cpu" } else { "name" }.into();
        self.ascending = n != 5;
        self.selected = None;
        self.selections.clear();
        self.scroll = 0.;
        self.hscroll = 0.;
        self.menus.clear();
        self.reset_columns();
        self.rebuild();
    }
    pub fn current(&self) -> Option<&Row> {
        self.selected
            .as_ref()
            .and_then(|id| self.rows.iter().find(|r| &r.id == id))
    }
    pub fn service_targets(&self, action: &str) -> Vec<&Service> {
        if self.page != 3 {
            return self
                .current()
                .and_then(|r| r.service)
                .map(|i| &self.services[i])
                .filter(|s| s.permits(action))
                .into_iter()
                .collect();
        }
        self.rows
            .iter()
            .filter(|r| self.selections.contains(&r.id) || self.selected.as_ref() == Some(&r.id))
            .filter_map(|r| r.service.map(|i| &self.services[i]))
            .filter(|s| s.permits(action))
            .collect()
    }

    pub fn process(&self) -> Option<&Process> {
        self.current()?
            .members
            .first()
            .and_then(|&i| self.records.get(i))
    }
    pub fn selected_processes(&self) -> Vec<&Process> {
        let mut seen = HashSet::new();
        self.rows
            .iter()
            .filter(|r| self.selections.contains(&r.id) || self.selected.as_ref() == Some(&r.id))
            .flat_map(|r| r.members.iter().map(|&i| &self.records[i]))
            .filter(|p| seen.insert(p.id()))
            .collect()
    }
    pub fn can_end(&self) -> bool {
        if self.page == 4 || (self.selected.is_none() && self.selections.is_empty()) {
            return false;
        }
        let mut selected = self
            .rows
            .iter()
            .filter(|row| {
                self.selected.as_ref() == Some(&row.id) || self.selections.contains(&row.id)
            })
            .flat_map(|row| row.members.iter())
            .peekable();
        selected.peek().is_some() && selected.all(|&i| !self.records[i].protected())
    }
    pub fn row_height(&self) -> f64 {
        if [5, 6, 3, 2].contains(&self.page) {
            24.
        } else {
            28.
        }
    }
    pub fn header_height(&self) -> f64 {
        if [0, 4].contains(&self.page) {
            56.
        } else {
            30.
        }
    }
    pub fn table_top(&self) -> f64 {
        96. + self.header_height() + if self.page == 2 { 46. } else { 0. }
    }
    pub fn visible_rows(&self) -> std::ops::Range<usize> {
        let height = self.row_height();
        let first = (self.scroll / height).floor() as usize;
        let count = ((self.height - self.table_top()) / height).ceil() as usize + 1;
        first.min(self.rows.len())..(first + count).min(self.rows.len())
    }

    pub fn metrics_left(&self) -> f64 {
        let names: f64 = self
            .columns
            .iter()
            .take_while(|c| !c.numeric)
            .map(|c| c.width)
            .sum();
        (self.sidebar() - self.hscroll + names).clamp(self.sidebar(), self.width)
    }

    pub fn max_scroll(&self) -> f64 {
        if self.page == 7 {
            return (self.settings_height - self.height).max(0.);
        }
        (self.rows.len() as f64 * self.row_height() - (self.height - self.table_top() - 15.))
            .max(0.)
    }
    pub fn clamp_scroll(&mut self) {
        self.scroll = self.scroll.clamp(0., self.max_scroll());
        let total: f64 = self.columns.iter().map(|c| c.width).sum();
        self.hscroll = self
            .hscroll
            .clamp(0., (total - (self.width - self.sidebar() - 15.)).max(0.));
    }
    fn process_row(&self, members: Vec<usize>, id: String, name: String, group: bool) -> Row {
        let first = &self.records[members[0]];
        let mut r = Row {
            name,
            group,
            icon: if id.ends_with(".app") {
                id.clone()
            } else {
                first.info.app.clone()
            },
            id,
            ..Default::default()
        };
        r.metrics = RowMetrics::for_processes(&self.records, &members);
        if first.raw.state == 4 && matches!(self.page, 0 | 4) {
            r.values.insert("status".into(), "Suspended".into());
        }
        r.members = members;
        r
    }

    pub fn number(&self, row: &Row, key: &str) -> Option<f64> {
        match key {
            "pid" => row.metrics.pid,
            "cpu" => row.metrics.cpu,
            "memory" => row.metrics.memory,
            "disk" => row.metrics.disk,
            "time" => row.metrics.time,
            "threads" => row.metrics.threads,
            "ppid" | "start" | "nice" => {
                let raw = &self.records[*row.members.first()?].raw;
                Some(match key {
                    "ppid" => f64::from(raw.ppid),
                    "nice" => f64::from(raw.nice),
                    _ => raw.start as f64,
                })
            }
            _ => None,
        }
    }

    fn row_text<'a>(&'a self, row: &'a Row, key: &str) -> Option<&'a str> {
        if key == "name" {
            return Some(&row.name);
        }
        if let Some(value) = row.values.get(key) {
            return Some(value);
        }
        let first = &self.records[*row.members.first()?];
        match key {
            "status" => Some(if self.page == 5 {
                first.status()
            } else if first.raw.state == 4 {
                "Suspended"
            } else {
                ""
            }),
            "user" => Some(&first.info.user),
            "description" => Some(&first.info.title),
            "path" => Some(&first.info.path),
            "bundle" => Some(&first.info.bundle),
            "arch" => match first.raw.arch {
                0x0100000c => Some("ARM64"),
                0x01000007 => Some("x64"),
                _ => None,
            },
            _ => None,
        }
    }

    pub fn row_value<'a>(&'a self, row: &'a Row, key: &str) -> std::borrow::Cow<'a, str> {
        let Some(number) = self.number(row, key) else {
            return std::borrow::Cow::Borrowed(self.row_text(row, key).unwrap_or("–"));
        };
        std::borrow::Cow::Owned(match key {
            "cpu" => format!("{number:0.1}%"),
            "memory" => {
                if self.memory_percent {
                    format!("{:0.1}%", number / self.system.raw.ram.max(1) as f64 * 100.)
                } else {
                    bytes(number, self.prefs.binary)
                }
            }
            "disk" => format!("{}/s", bytes(number, self.prefs.binary)),
            "time" => duration(number),
            _ => format!("{number:0.0}"),
        })
    }
    fn compare_rows(&self, a: &Row, b: &Row) -> std::cmp::Ordering {
        let key = &self.sort;
        let order = match (self.number(a, key), self.number(b, key)) {
            (Some(a), Some(b)) => a.total_cmp(&b),
            (None, Some(_)) => return std::cmp::Ordering::Greater,
            (Some(_), None) => return std::cmp::Ordering::Less,
            _ => {
                let a = self.row_text(a, key).unwrap_or_default();
                let b = self.row_text(b, key).unwrap_or_default();
                if a.is_ascii() && b.is_ascii() {
                    a.bytes()
                        .map(|c| c.to_ascii_lowercase())
                        .cmp(b.bytes().map(|c| c.to_ascii_lowercase()))
                } else {
                    a.to_lowercase().cmp(&b.to_lowercase())
                }
            }
        };
        (if self.ascending {
            order
        } else {
            order.reverse()
        })
        .then_with(|| a.id.cmp(&b.id))
    }

    /// Installs the new snapshot. Returns true when its process rows are already up to date.
    pub fn replace_processes(&mut self, records: Vec<Process>, regular_apps_changed: bool) -> bool {
        let reusable = matches!(self.page, 0 | 4 | 5)
            && !regular_apps_changed
            && records.len() == self.records.len()
            && records.iter().zip(&self.records).all(|(new, old)| {
                let a = &new.raw;
                let b = &old.raw;
                a.pid == b.pid
                    && a.start == b.start
                    && a.micro == b.micro
                    && a.ppid == b.ppid
                    && a.uid == b.uid
                    && a.arch == b.arch
                    && (a.state == 4) == (b.state == 4)
                    && (Arc::ptr_eq(&new.info, &old.info)
                        || (new.info.name == old.info.name
                            && new.info.app == old.info.app
                            && new.info.title == old.info.title
                            && new.info.path == old.info.path
                            && new.info.bundle == old.info.bundle
                            && new.info.user == old.info.user))
            });
        self.records = records;
        if reusable {
            for row in self.rows.iter_mut().filter(|row| !row.members.is_empty()) {
                row.metrics = RowMetrics::for_processes(&self.records, &row.members);
            }
            if matches!(
                self.sort.as_str(),
                "cpu" | "memory" | "disk" | "status" | "threads" | "time" | "nice"
            ) {
                self.sort_rows();
            }
        }
        reusable
    }

    pub fn rebuild(&mut self) {
        let q = self.query.trim().to_lowercase();
        let mut rows = vec![];
        match self.page {
            0 | 5 => {
                if self.page == 0 && self.prefs.grouped {
                    for (key, name, members) in group_processes(&self.records) {
                        if !members.iter().any(|&i| self.records[i].matches(&q)) {
                            continue;
                        }
                        let expandable = members.len() > 1;
                        rows.push(self.process_row(members, key, name, expandable));
                    }
                } else {
                    for (i, p) in self
                        .records
                        .iter()
                        .enumerate()
                        .filter(|(_, p)| p.matches(&q))
                    {
                        rows.push(self.process_row(
                            vec![i],
                            p.id(),
                            if self.page == 5 {
                                p.info.name.clone()
                            } else {
                                p.info.title.clone()
                            },
                            false,
                        ));
                    }
                }
            }
            2 => {
                for (k, h) in &self.history.entries {
                    if (!self.prefs.history_all
                        && !(h.path.starts_with("/Applications/")
                            || h.path.starts_with("/System/Applications/")))
                        || !h.name.to_lowercase().contains(&q)
                    {
                        continue;
                    }
                    let mut r = Row {
                        id: k.clone(),
                        name: h.name.clone(),
                        icon: h.path.clone(),
                        ..Default::default()
                    };
                    r.metrics.time = Some(h.cpu);
                    rows.push(r);
                }
            }
            3 | 6 => {
                for (i, s) in self.services.iter().enumerate() {
                    let startup = s.startup.as_ref().filter(|_| self.page == 3);
                    if (self.page == 3 && startup.is_none())
                        || (self.page == 6 && s.domain == "login")
                    {
                        continue;
                    }
                    let name = startup.map_or(s.label.as_str(), |item| &item.name);
                    if ![
                        name,
                        &s.label,
                        &s.path,
                        &s.program,
                        &s.publisher,
                        &s.pid.unwrap_or_default().to_string(),
                    ]
                    .iter()
                    .any(|v| v.to_lowercase().contains(&q))
                    {
                        continue;
                    }
                    let mut r = Row {
                        id: s.id(),
                        name: name.into(),
                        service: Some(i),
                        icon: if self.page == 6 {
                            "service.item".into()
                        } else {
                            s.program.clone()
                        },
                        ..Default::default()
                    };
                    if let Some(p) = s.pid {
                        r.metrics.pid = Some(f64::from(p));
                    }
                    r.values.insert(
                        "status".into(),
                        if self.page == 3 {
                            s.startup_status()
                        } else {
                            s.status()
                        }
                        .into(),
                    );
                    r.values.insert("publisher".into(), s.publisher.clone());
                    r.values.insert("description".into(), s.program.clone());
                    r.values.insert("group".into(), s.domain.clone());
                    r.values.insert("impact".into(), "Not measured".into());
                    if let Some(item) = startup {
                        r.values.insert("kind".into(), item.description().into());
                    }
                    rows.push(r);
                }
            }
            4 => {
                let mut groups: HashMap<i32, Vec<usize>> = HashMap::new();
                for (i, p) in self.records.iter().enumerate() {
                    if p.raw.uid >= 500 {
                        groups.entry(p.raw.uid).or_default().push(i)
                    }
                }
                for (uid, m) in groups {
                    let name = self.records[m[0]].info.user.clone();
                    if !name.to_lowercase().contains(&q) {
                        continue;
                    }
                    let mut r = self.process_row(m, format!("user:{uid}"), name, true);
                    r.values.insert(
                        "status".into(),
                        if uid == unsafe { libc::getuid() } as i32 {
                            "Active"
                        } else {
                            ""
                        }
                        .into(),
                    );
                    rows.push(r);
                }
            }
            _ => {}
        }
        rows.sort_unstable_by(|a, b| self.compare_rows(a, b));
        if self.page == 0 && self.prefs.grouped {
            let (mut apps, mut background, mut system) = (vec![], vec![], vec![]);
            let uid = unsafe { libc::getuid() } as i32;
            for r in rows {
                if self.regular_apps.contains(&r.id) {
                    apps.push(r)
                } else if self.records[r.members[0]].raw.uid == uid {
                    background.push(r)
                } else {
                    system.push(r)
                }
            }
            rows = vec![];
            for (name, group) in [
                ("Apps", apps),
                ("Background processes", background),
                ("System processes", system),
            ] {
                if !group.is_empty() {
                    rows.push(Row::section(&format!("{name} ({})", group.len())));
                    rows.extend(group);
                }
            }
        }
        self.rows = if self.expanded.is_empty() {
            rows
        } else {
            let mut expanded = Vec::with_capacity(rows.len());
            for row in rows {
                let open = row.group && self.expanded.contains(&row.id);
                expanded.push(row);
                if open {
                    let parent = expanded.len() - 1;
                    for position in 0..expanded[parent].members.len() {
                        let i = expanded[parent].members[position];
                        let p = &self.records[i];
                        let mut child = self.process_row(
                            vec![i],
                            format!("child:{}", p.id()),
                            p.info.name.clone(),
                            false,
                        );
                        child.indent = true;
                        expanded.push(child);
                    }
                }
            }
            expanded
        };
        self.selections
            .retain(|id| self.rows.iter().any(|r| &r.id == id));
        if self
            .selected
            .as_ref()
            .is_some_and(|id| !self.rows.iter().any(|r| &r.id == id))
        {
            self.selected = None;
        }
        self.clamp_scroll();
    }
    pub fn selected_visible(&mut self) {
        if let Some(i) = self
            .selected
            .as_ref()
            .and_then(|id| self.rows.iter().position(|r| &r.id == id))
        {
            let y = i as f64 * self.row_height();
            let h = self.height - self.table_top() - 15.;
            if y < self.scroll {
                self.scroll = y;
            } else if y + self.row_height() > self.scroll + h {
                self.scroll = y + self.row_height() - h;
            }
            self.clamp_scroll();
        }
    }
}
pub fn workers(shared: Arc<Feed>) {
    let samples = shared.clone();
    std::thread::spawn(move || {
        let mut sampler = Sampler::default();
        let mut last_start: Option<Instant> = None;
        let mut state = samples.data.lock().unwrap();
        while !state.stop {
            let speed = state.speed;
            let interval = refresh_interval(speed);
            let next = interval.and_then(|d| last_start.map(|t| t + d));
            if std::mem::take(&mut state.reset_sample) {
                sampler.reset();
            }
            if state.refresh || next.is_some_and(|t| Instant::now() >= t) {
                state.refresh = false;
                let with_threads = state.with_threads;
                drop(state);
                let start = Instant::now();
                let sample = sampler.sample_with_threads(with_threads);
                state = samples.data.lock().unwrap();
                if state.speed == speed
                    && !state.reset_sample
                    && (!state.with_threads || with_threads)
                {
                    state.latest = Some(sample);
                    drop(state);
                    samples.notify_ui();
                    state = samples.data.lock().unwrap();
                }
                last_start = Some(start);
            } else {
                state = samples.wait(state, next);
            }
        }
    });
    std::thread::spawn(move || {
        let mut next = Instant::now();
        let mut state = shared.data.lock().unwrap();
        while !state.stop {
            let active = state.services_visible && state.speed != 0;
            if state.scan || (active && Instant::now() >= next) {
                state.scan = false;
                let startup = state.startup_visible;
                drop(state);
                let mut services = crate::services::scan(startup);
                if !startup {
                    for item in &mut services {
                        item.startup = None;
                    }
                }
                state = shared.data.lock().unwrap();
                if state.startup_visible != startup && state.services_visible {
                    continue;
                }
                state.services = Some(services);
                drop(state);
                shared.notify_ui();
                next = Instant::now() + Duration::from_secs(if startup { 60 } else { 15 });
                state = shared.data.lock().unwrap();
            } else {
                state = shared.wait(state, active.then_some(next));
            }
        }
    });
}

pub fn benchmark_model() {
    let mut sampler = Sampler::default();
    let sample = sampler.sample();
    let mut state = State::new();
    state.records = sample.processes;
    state.system = sample.system;
    let mut rows = vec![];
    for page in [0, 4, 5] {
        for key in ["name", "cpu", "memory", "disk", "network"] {
            state.set_page(page);
            state.sort = key.to_string();
            let mut ms = vec![];
            for _ in 0..100 {
                let start = Instant::now();
                state.ascending = !state.ascending;
                state.rebuild();
                ms.push(start.elapsed().as_secs_f64() * 1000.0);
            }
            ms.sort_by(f64::total_cmp);
            rows.push(serde_json::json!({"page":PAGES[page],"sort":key,"rows":state.rows.len(),"p50_ms":ms[50],"p95_ms":ms[95],"max_ms":ms[99]}));
        }
    }
    println!("{}", serde_json::to_string_pretty(&serde_json::json!({"process_count":state.records.len(),"measurement":"model rebuild only; excludes rendering and compositor", "results":rows})).unwrap());
}

impl State {
    /// Header clicks reorder the prepared rows without reformatting every metric.
    /// Preserve expansion and selection identities while keeping section headings fixed.
    pub fn sort_rows(&mut self) {
        if !self.rows.iter().any(|row| row.indent) {
            let mut rows = std::mem::take(&mut self.rows);
            for section in rows.split_mut(|row| row.section) {
                section.sort_unstable_by(|a, b| self.compare_rows(a, b));
            }
            self.rows = rows;
            self.clamp_scroll();
            return;
        }
        let mut tops = Vec::new();
        let mut children: HashMap<String, Vec<Row>> = HashMap::new();
        let mut parent = String::new();
        for row in std::mem::take(&mut self.rows) {
            if row.section {
                continue;
            }
            if row.indent {
                children.entry(parent.clone()).or_default().push(row);
            } else {
                parent = row.id.clone();
                tops.push(row);
            }
        }
        tops.sort_unstable_by(|a, b| self.compare_rows(a, b));
        if self.page == 0 && self.prefs.grouped {
            let mut groups: [Vec<Row>; 3] = Default::default();
            let uid = unsafe { libc::getuid() } as i32;
            for row in tops {
                let group = if self.regular_apps.contains(&row.id) {
                    0
                } else if row
                    .members
                    .first()
                    .is_some_and(|&i| self.records[i].raw.uid == uid)
                {
                    1
                } else {
                    2
                };
                groups[group].push(row);
            }
            for (name, group) in ["Apps", "Background processes", "System processes"]
                .into_iter()
                .zip(groups)
            {
                if group.is_empty() {
                    continue;
                }
                self.rows
                    .push(Row::section(&format!("{name} ({})", group.len())));
                for row in group {
                    let child = children.remove(&row.id).unwrap_or_default();
                    self.rows.push(row);
                    self.rows.extend(child);
                }
            }
        } else {
            for row in tops {
                let child = children.remove(&row.id).unwrap_or_default();
                self.rows.push(row);
                self.rows.extend(child);
            }
        }
        self.clamp_scroll();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn next_sample(feed: &Feed) -> Sample {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(sample) = feed.data.lock().unwrap().latest.take() {
                return sample;
            }
            assert!(Instant::now() < deadline, "sampler did not deliver");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn operation_is_available_before_delivery_notification() {
        let feed = State::new().shared;
        let weak = Arc::downgrade(&feed);
        let (sent, received) = std::sync::mpsc::channel();
        assert!(feed
            .on_delivery
            .set(Box::new(move || {
                let feed = weak.upgrade().unwrap();
                let ready = feed.data.try_lock().unwrap().results.len();
                sent.send(ready).unwrap();
            }))
            .is_ok());
        feed.deliver(Delivery::Operation(Ok(String::new())));
        assert_eq!(received.try_recv(), Ok(1));
    }

    #[test]
    fn live_refresh_pause_manual_and_resume() {
        let state = State::new();
        let feed = &state.shared;
        {
            let mut shared = feed.data.lock().unwrap();
            shared.speed = 1;
            shared.scan = false;
            shared.services_visible = false;
        }
        workers(feed.clone());
        let first = next_sample(feed);
        let second = next_sample(feed);
        let interval = second.at.duration_since(first.at);
        assert!(interval >= Duration::from_millis(245));
        assert!(interval < Duration::from_secs(1));
        feed.set_speed(0);
        std::thread::sleep(Duration::from_millis(350));
        assert!(feed.data.lock().unwrap().latest.is_none());
        feed.data.lock().unwrap().refresh = true;
        feed.wake.notify_all();
        let manual = next_sample(feed);
        assert!(!manual.processes.is_empty());
        assert!(manual.system.elapsed > 0.);
        assert!(feed.data.lock().unwrap().services.is_none());
        feed.set_speed(2);
        let resumed = next_sample(feed);
        assert_eq!(resumed.system.elapsed, 0.);
        assert!(next_sample(feed).at.duration_since(resumed.at) >= Duration::from_millis(495));
        feed.stop();
    }

    #[test]
    fn every_table_column_has_sortable_identity() {
        for page in [0, 2, 3, 4, 5, 6] {
            let cols = columns(page);
            assert!(!cols.is_empty());
            let ids: HashSet<_> = cols.iter().map(|c| &c.key).collect();
            assert_eq!(ids.len(), cols.len());
            assert!(cols.iter().all(|c| c.width >= 55.));
        }
    }
    fn fixture_processes() -> Vec<Process> {
        let template = Sampler::default()
            .sample()
            .processes
            .into_iter()
            .find(|p| p.raw.pid == unsafe { libc::getpid() })
            .unwrap();
        [
            (10, 1, "/Applications/Editor.app"),
            (11, 10, ""),
            (12, 1, ""),
        ]
        .into_iter()
        .map(|(pid, parent, app)| {
            let mut process = template.clone();
            process.raw.pid = pid;
            process.raw.uid = 501;
            process.raw.ppid = parent;
            process.raw.state = 2;
            process.raw.threads = -1;
            process.raw.valid = 1;
            process.raw.io_valid = 1;
            process.cpu = f64::from(pid);
            let info = Arc::make_mut(&mut process.info);
            info.name = format!("process-{pid}");
            info.path = format!("/tmp/{pid}");
            info.app = app.into();
            info.user = "tester".into();
            info.title = format!("Title-{pid}");
            process
        })
        .collect()
    }

    #[test]
    fn retained_rows_match_rebuild_for_sorted_filtered_and_expanded_tables() {
        let records = fixture_processes();
        let mut next = records.clone();
        next[0].cpu = 0.;
        next[1].raw.memory += 4096;
        next[2].cpu = 99.;
        next[2].raw.threads = 7;
        next[2].raw.state = 3;
        for (page, sort, query) in [
            (0, "name", "tester"),
            (0, "cpu", ""),
            (4, "memory", "tester"),
            (5, "status", ""),
        ] {
            let mut state = State::new();
            state.prefs.grouped = true;
            state.records = records.clone();
            state.set_page(page);
            state.sort = sort.into();
            state.query = query.into();
            state.regular_apps.insert("/Applications/Editor.app".into());
            state.expanded.insert("/Applications/Editor.app".into());
            state
                .expanded
                .insert(format!("user:{}", records[0].raw.uid));
            state.rebuild();
            assert!(state.rows.iter().any(|r| !r.members.is_empty()));
            let selected = state
                .rows
                .iter()
                .find(|r| !r.members.is_empty())
                .unwrap()
                .id
                .clone();
            state.selections.insert(selected.clone());
            state.selected = Some(selected.clone());
            assert!(state.replace_processes(next.clone(), false));
            let retained = state.rows.clone();
            state.rebuild();
            assert_eq!(retained, state.rows);
            assert_eq!(state.selected.as_ref(), Some(&selected));
            assert!(state.selections.contains(&selected));
            assert!(state
                .rows
                .iter()
                .any(|r| r.members.contains(&2) && r.metrics.threads == Some(7.)));
            assert!(state
                .rows
                .iter()
                .filter(|r| !r.members.contains(&2))
                .all(|r| r.metrics.threads.is_none()));
        }
    }

    #[test]
    fn changed_membership_metadata_or_order_requires_rebuild() {
        let records = fixture_processes();
        let mut state = State::new();
        state.records = records.clone();
        state.set_page(0);
        let mut reused = records.clone();
        reused[0].raw.micro += 1;
        let mut reparented = records.clone();
        reparented[1].raw.ppid = 1;
        let mut renamed = records.clone();
        Arc::make_mut(&mut renamed[0].info).title = "Updated editor".into();
        let mut reordered = records.clone();
        reordered.swap(0, 1);
        for next in [reused, reparented, renamed, reordered] {
            state.records = records.clone();
            state.rebuild();
            assert!(!state.replace_processes(next, false));
        }
        state.records = records.clone();
        state.rebuild();
        assert!(!state.replace_processes(records, true));
    }

    #[test]
    fn cached_sort_preserves_selection_and_unknown_last() {
        let mut s = State::new();
        s.page = 5;
        s.rows = vec![
            Row {
                id: "a".into(),
                name: "a".into(),
                metrics: RowMetrics {
                    cpu: Some(2.),
                    ..Default::default()
                },
                ..Default::default()
            },
            Row {
                id: "b".into(),
                name: "b".into(),
                metrics: RowMetrics {
                    cpu: Some(8.),
                    ..Default::default()
                },
                ..Default::default()
            },
            Row {
                id: "unknown".into(),
                name: "unknown".into(),
                ..Default::default()
            },
        ];
        s.sort = "cpu".into();
        s.ascending = false;
        s.selected = Some("a".into());
        s.sort_rows();
        assert_eq!(s.rows[0].id, "b");
        assert_eq!(s.rows[2].id, "unknown");
        assert_eq!(s.selected.as_deref(), Some("a"));
        s.ascending = true;
        s.sort_rows();
        assert_eq!(s.rows[0].id, "a");
        assert_eq!(s.rows[2].id, "unknown");
    }
    #[test]
    fn no_negative_scroll() {
        let mut s = State::new();
        s.scroll = -100.;
        s.hscroll = -50.;
        s.clamp_scroll();
        assert_eq!(s.scroll, 0.);
        assert_eq!(s.hscroll, 0.);
    }
    #[test]
    fn navigation_resets_selection() {
        let mut s = State::new();
        s.selected = Some("a".into());
        s.selections.insert("a".into());
        s.set_page(6);
        assert!(s.selected.is_none());
        assert!(s.selections.is_empty());
    }
}
