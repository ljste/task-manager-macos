//! Owned monitoring model. No AppKit objects cross the sampler thread boundary.
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    ffi::c_void,
    sync::Arc,
    time::{Duration, Instant},
};
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ProcessStats {
    pub pid: i32,
    pub ppid: i32,
    pub uid: i32,
    pub state: i32,
    pub threads: i32,
    pub nice: i32,
    pub arch: i32,
    pub valid: i32,
    pub io_valid: i32,
    pub cpu_ns: u64,
    pub memory: u64,
    pub read: u64,
    pub write: u64,
    pub start: u64,
    pub micro: u64,
}

#[repr(C)]
pub struct RawProcess {
    pub stats: ProcessStats,
    pub name: [i8; 256],
    pub path: [i8; 4096],
    pub user: [i8; 256],
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct RawSystem {
    pub user: u64,
    pub system: u64,
    pub idle: u64,
    pub nice: u64,
    pub ram: u64,
    pub active: u64,
    pub inactive: u64,
    pub wired: u64,
    pub compressed: u64,
    pub free: u64,
    pub purgeable: u64,
    pub speculative: u64,
    pub swap_used: u64,
    pub swap_total: u64,
    pub disk_read: u64,
    pub disk_write: u64,
    pub net_in: u64,
    pub net_out: u64,
    pub capacity: u64,
    pub disk_free: u64,
    pub disk_valid: i32,
    pub pressure: i32,
    pub logical: i32,
    pub physical: i32,
    pub performance: i32,
    pub efficiency: i32,
    pub uptime: f64,
    pub load1: f64,
    pub load5: f64,
    pub load15: f64,
    pub brand: [i8; 256],
    pub filesystem: [i8; 64],
    pub mount: [i8; 256],
    pub device: [i8; 256],
    pub interfaces: [i8; 2048],
}
impl Default for RawSystem {
    fn default() -> Self {
        unsafe { std::mem::zeroed() }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Core {
    pub user: u32,
    pub system: u32,
    pub idle: u32,
    pub nice: u32,
}
extern "C" {
    fn tm_system(out: *mut RawSystem);
    fn tm_visit_processes(
        with_threads: i32,
        context: *mut c_void,
        visit: extern "C" fn(*mut c_void, &RawProcess, &[u8; 16], u64),
    );
    fn tm_sample_image(
        process: &RawProcess,
        path: *mut i8,
        capacity: i32,
        name: *mut i8,
        arch: *mut i32,
    ) -> i32;
    fn tm_username(uid: i32, out: *mut i8, capacity: i32);
    fn tm_cpu_load(out: *mut *mut Core) -> i32;
    fn tm_free(p: *mut std::ffi::c_void);
    pub(crate) fn tm_read_process(pid: i32, out: *mut RawProcess) -> i32;
    pub fn tm_open_files(pid: i32) -> i32;
    pub fn tm_arguments(pid: i32, out: *mut i8, n: i32) -> i32;
}
pub fn cstr(s: &[i8]) -> String {
    let n = s.iter().position(|&x| x == 0).unwrap_or(s.len());
    let bytes = unsafe { std::slice::from_raw_parts(s.as_ptr().cast::<u8>(), n) };
    String::from_utf8_lossy(bytes).into_owned()
}
pub fn rate(n: u64, p: u64, dt: f64) -> f64 {
    if dt > 0.0 {
        n.saturating_sub(p) as f64 / dt
    } else {
        0.0
    }
}
impl Core {
    pub fn delta(self, p: Self) -> Option<(f64, f64)> {
        let u = self.user.wrapping_sub(p.user) as u64;
        let s = self.system.wrapping_sub(p.system) as u64;
        let i = self.idle.wrapping_sub(p.idle) as u64;
        let n = self.nice.wrapping_sub(p.nice) as u64;
        let total = u + s + i + n;
        (total > 0).then(|| {
            (
                (u + s + n) as f64 / total as f64 * 100.0,
                s as f64 / total as f64 * 100.0,
            )
        })
    }
}
#[derive(Clone)]
pub struct ProcessInfo {
    short_name: [i8; 17],
    start_ticks: u64,
    uuid: [u8; 16],
    arch: i32,
    pub name: String,
    pub path: String,
    pub user: String,
    pub app: String,
    pub title: String,
    pub bundle: String,
}

#[derive(Clone)]
pub struct Process {
    pub info: Arc<ProcessInfo>,
    pub raw: ProcessStats,
    pub cpu: f64,
    pub read: f64,
    pub write: f64,
}
impl Process {
    pub fn id(&self) -> String {
        format!("{}:{}:{}", self.raw.pid, self.raw.start, self.raw.micro)
    }
    pub fn status(&self) -> &str {
        match self.raw.state {
            1 => "Idle",
            2 => "Running",
            3 => "Sleeping",
            4 => "Stopped",
            5 => "Zombie",
            _ => "Unknown",
        }
    }
    pub fn protected(&self) -> bool {
        self.raw.pid <= 1
            || self.raw.pid == unsafe { libc::getpid() }
            || self.raw.uid != unsafe { libc::getuid() } as i32
            || [
                "WindowServer",
                "loginwindow",
                "launchd",
                "kernel_task",
                "watchdogd",
                "powerd",
                "securityd",
                "opendirectoryd",
                "runningboardd",
                "systemstats",
                "coreservicesd",
                "tccd",
            ]
            .contains(&self.info.name.as_str())
            || ["/System/", "/usr/libexec/", "/usr/sbin/"]
                .iter()
                .any(|p| self.info.path.starts_with(p))
    }
    pub fn matches(&self, q: &str) -> bool {
        q.is_empty()
            || [
                &self.info.name,
                &self.info.path,
                &self.info.user,
                &self.info.title,
                &self.info.bundle,
                &self.raw.pid.to_string(),
            ]
            .iter()
            .any(|v| v.to_lowercase().contains(q))
    }
    pub fn revalidate(&self) -> Result<(), String> {
        let mut now: RawProcess = unsafe { std::mem::zeroed() };
        if unsafe { tm_read_process(self.raw.pid, &mut now) } == 0
            || now.stats.start != self.raw.start
            || now.stats.micro != self.raw.micro
            || now.stats.uid != self.raw.uid
            || cstr(&now.path) != self.info.path
            || cstr(&now.name) != self.info.name
        {
            return Err("The process exited or its PID was reused. Refresh and try again.".into());
        }
        if self.protected() {
            return Err("This process is protected or belongs to another user.".into());
        }
        Ok(())
    }
    pub fn signal(&self, sig: i32) -> Result<(), String> {
        self.revalidate()?;
        if unsafe { libc::kill(self.raw.pid, sig) } != 0 {
            Err(std::io::Error::last_os_error().to_string())
        } else {
            Ok(())
        }
    }
    pub fn priority(&self, nice: i32) -> Result<(), String> {
        self.revalidate()?;
        if ![0, 10, 19].contains(&nice) || nice < self.raw.nice {
            return Err("Only lowering this process's priority is supported.".into());
        }
        if unsafe { libc::setpriority(libc::PRIO_PROCESS, self.raw.pid as u32, nice) } != 0 {
            Err(std::io::Error::last_os_error().to_string())
        } else {
            Ok(())
        }
    }
}
#[derive(Clone, Default)]
pub struct Snapshot {
    pub raw: RawSystem,
    pub cpu: f64,
    pub read: f64,
    pub write: f64,
    pub sent: f64,
    pub received: f64,
    pub cores: Vec<Option<(f64, f64)>>,
    pub elapsed: f64,
    pub sample_ms: f64,
}
impl Snapshot {
    pub fn used(&self) -> u64 {
        (self.raw.active + self.raw.wired + self.raw.compressed).min(self.raw.ram)
    }
    pub fn memory(&self) -> f64 {
        if self.raw.ram > 0 {
            self.used() as f64 / self.raw.ram as f64 * 100.0
        } else {
            0.0
        }
    }
}
pub struct GraphSample {
    pub at: Instant,
    pub continuous: bool,
    pub cpu: f64,
    pub memory: f64,
    pub disk: Option<(f64, f64)>,
    pub sent: f64,
    pub received: f64,
    pub cores: Vec<Option<(f64, f64)>>,
}

pub fn record_graph(history: &mut VecDeque<GraphSample>, at: Instant, system: &Snapshot) {
    let window = Duration::from_secs(300);
    while history
        .front()
        .is_some_and(|first| at.duration_since(first.at) > window)
    {
        history.pop_front();
    }
    let continuous = system.elapsed > 0.
        && history
            .back()
            .is_some_and(|last| at.duration_since(last.at).as_secs_f64() <= system.elapsed + 0.001);
    push_history(
        history,
        GraphSample {
            at,
            continuous,
            cpu: system.cpu,
            memory: system.memory(),
            disk: (system.raw.disk_valid != 0).then_some((system.read, system.write)),
            sent: system.sent,
            received: system.received,
            cores: system.cores.clone(),
        },
        1201,
    );
}

pub struct Sample {
    pub at: Instant,
    pub processes: Vec<Process>,
    pub system: Snapshot,
}
struct Baseline {
    info: Arc<ProcessInfo>,
    sampled: Instant,
    checked: Instant,
    uid: i32,
    cpu_ns: Option<u64>,
    io: Option<(u64, u64)>,
}

#[derive(Default)]
pub struct Sampler {
    previous: HashMap<(i32, u64, u64), Baseline>,
    users: HashMap<i32, String>,
    system: Option<RawSystem>,
    cores: Vec<Core>,
    time: Option<Instant>,
    bundles: HashMap<String, (String, String)>,
}
impl ProcessStats {
    fn identity(&self) -> (i32, u64, u64) {
        (self.pid, self.start, self.micro)
    }
}

struct ProcessBatch<'a> {
    sampler: &'a mut Sampler,
    processes: Vec<Process>,
    elapsed: f64,
    logical: i32,
    now: Instant,
}

extern "C" fn collect_process(
    context: *mut c_void,
    process: &RawProcess,
    uuid: &[u8; 16],
    start_ticks: u64,
) {
    // The bridge calls synchronously with the batch borrowed by Sampler::sample.
    let batch = unsafe { &mut *context.cast::<ProcessBatch>() };
    batch.push(process, uuid, start_ticks);
}

impl ProcessBatch<'_> {
    fn read_info(
        &mut self,
        r: &RawProcess,
        uuid: [u8; 16],
        start_ticks: u64,
        short_name: [i8; 17],
    ) -> Option<Arc<ProcessInfo>> {
        let mut buffer = [0; 4096];
        let mut full_name = [0; 256];
        let mut arch = 0;
        if unsafe {
            tm_sample_image(
                r,
                buffer.as_mut_ptr(),
                buffer.len() as i32,
                full_name.as_mut_ptr(),
                &mut arch,
            )
        } == 0
        {
            return None;
        }
        let path = cstr(&buffer);
        let name = cstr(&full_name);
        if let Some(old) = self.sampler.previous.get(&r.stats.identity()) {
            let info = &old.info;
            if old.uid == r.stats.uid
                && info.start_ticks == start_ticks
                && info.uuid == uuid
                && info.short_name == short_name
                && info.arch == arch
                && info.path == path
                && info.name == name
            {
                return Some(info.clone());
            }
        }
        let app = path
            .find(".app/")
            .map(|i| path[..i + 4].to_string())
            .unwrap_or_default();
        let (title, bundle) = if app.is_empty() {
            (name.clone(), String::new())
        } else {
            self.sampler
                .bundles
                .entry(app.clone())
                .or_insert_with(|| {
                    let v = plist::Value::from_file(format!("{app}/Contents/Info.plist")).ok();
                    let d = v.as_ref().and_then(|v| v.as_dictionary());
                    let get = |k: &str| {
                        d.and_then(|d| d.get(k))
                            .and_then(|v| v.as_string())
                            .map(str::to_string)
                    };
                    (
                        get("CFBundleDisplayName")
                            .or_else(|| get("CFBundleName"))
                            .unwrap_or_else(|| {
                                std::path::Path::new(&app)
                                    .file_stem()
                                    .unwrap()
                                    .to_string_lossy()
                                    .into_owned()
                            }),
                        get("CFBundleIdentifier").unwrap_or_default(),
                    )
                })
                .clone()
        };
        Some(Arc::new(ProcessInfo {
            short_name,
            start_ticks,
            uuid,
            arch,
            name,
            path,
            user: self
                .sampler
                .users
                .entry(r.stats.uid)
                .or_insert_with(|| {
                    let mut username = [0; 256];
                    unsafe {
                        tm_username(r.stats.uid, username.as_mut_ptr(), username.len() as i32)
                    };
                    cstr(&username)
                })
                .clone(),
            app,
            title,
            bundle,
        }))
    }

    fn push(&mut self, r: &RawProcess, uuid: &[u8; 16], start_ticks: u64) {
        let key = r.stats.identity();
        let old = self.sampler.previous.get(&key);
        if old.is_some_and(|old| {
            old.info.start_ticks != 0 && start_ticks != 0 && old.info.start_ticks != start_ticks
        }) {
            // PID reuse between the bulk list and the per-PID counters.
            return;
        }
        let short_name = r.name[..17].try_into().unwrap();
        let cached = old.filter(|old| {
            old.uid == r.stats.uid
                && old.info.start_ticks == start_ticks
                && &old.info.uuid == uuid
                && old.info.short_name == short_name
                && self.now.duration_since(old.checked) < Duration::from_secs(4)
        });
        let cpu_ns = (r.stats.valid != 0).then_some(r.stats.cpu_ns);
        let io = (r.stats.io_valid != 0).then_some((r.stats.read, r.stats.write));
        let cpu = cpu_ns
            .zip(old.and_then(|p| p.cpu_ns))
            .map_or(0., |(new, old)| {
                (rate(new, old, self.elapsed) / 1e9 / self.logical.max(1) as f64 * 100.)
                    .clamp(0., 100.)
            });
        let (read, write) = io
            .zip(old.and_then(|p| p.io))
            .map_or((0., 0.), |(new, old)| {
                (
                    rate(new.0, old.0, self.elapsed),
                    rate(new.1, old.1, self.elapsed),
                )
            });
        let (info, checked) = match cached {
            Some(old) => (old.info.clone(), old.checked),
            None => {
                let Some(info) = self.read_info(r, *uuid, start_ticks, short_name) else {
                    return;
                };
                (info, self.now)
            }
        };
        self.sampler.previous.insert(
            key,
            Baseline {
                info: info.clone(),
                sampled: self.now,
                checked,
                uid: r.stats.uid,
                cpu_ns,
                io,
            },
        );
        let p = Process {
            raw: ProcessStats {
                arch: info.arch,
                ..r.stats
            },
            info,
            cpu,
            read,
            write,
        };
        self.processes.push(p);
    }
}

impl Sampler {
    pub fn reset(&mut self) {
        self.previous.clear();
        self.system = None;
        self.cores.clear();
        self.time = None;
    }
    pub fn sample(&mut self) -> Sample {
        self.sample_with_threads(true)
    }

    pub fn sample_with_threads(&mut self, with_threads: bool) -> Sample {
        let now = Instant::now();
        let dt = self
            .time
            .map(|t| now.duration_since(t).as_secs_f64())
            .unwrap_or(0.0);
        let mut raw = RawSystem::default();
        unsafe { tm_system(&mut raw) };
        let mut batch = ProcessBatch {
            processes: Vec::with_capacity(self.previous.len()),
            sampler: self,
            elapsed: dt,
            logical: raw.logical,
            now,
        };
        unsafe {
            tm_visit_processes(
                i32::from(with_threads),
                (&mut batch as *mut ProcessBatch).cast(),
                collect_process,
            )
        };
        let processes = batch.processes;
        let mut cp = std::ptr::null_mut();
        let nc = unsafe { tm_cpu_load(&mut cp) };
        let cores = if !cp.is_null() {
            let v = unsafe { std::slice::from_raw_parts(cp, nc.max(0) as usize) }.to_vec();
            unsafe { tm_free(cp.cast()) }
            v
        } else {
            Vec::new()
        };
        let values = cores
            .iter()
            .enumerate()
            .map(|(i, c)| {
                if self.cores.len() == cores.len() {
                    c.delta(self.cores[i])
                } else {
                    None
                }
            })
            .collect();
        let mut sys = Snapshot {
            raw,
            cores: values,
            elapsed: dt,
            ..Default::default()
        };
        if let Some(old) = self.system {
            let a = Core {
                user: raw.user as u32,
                system: raw.system as u32,
                idle: raw.idle as u32,
                nice: raw.nice as u32,
            };
            let b = Core {
                user: old.user as u32,
                system: old.system as u32,
                idle: old.idle as u32,
                nice: old.nice as u32,
            };
            sys.cpu = a.delta(b).map(|x| x.0).unwrap_or(0.0);
            sys.read = rate(raw.disk_read, old.disk_read, dt);
            sys.write = rate(raw.disk_write, old.disk_write, dt);
            sys.sent = rate(raw.net_out, old.net_out, dt);
            sys.received = rate(raw.net_in, old.net_in, dt);
        }
        if self.previous.len() != processes.len() {
            self.previous.retain(|_, previous| previous.sampled == now);
        }
        self.system = Some(raw);
        self.cores = cores;
        self.time = Some(now);
        let live: HashSet<_> = processes.iter().map(|p| &p.info.app).collect();
        self.bundles.retain(|k, _| live.contains(k));
        sys.sample_ms = now.elapsed().as_secs_f64() * 1000.0;
        Sample {
            at: now,
            processes,
            system: sys,
        }
    }
}
#[derive(Clone, Serialize, Deserialize, Default)]
pub struct History {
    pub name: String,
    pub path: String,
    pub cpu: f64,
    pub peak: u64,
    pub seconds: f64,
    pub disk: f64,
}
#[derive(Default)]
pub struct HistoryStore {
    pub entries: HashMap<String, History>,
    last: HashMap<(i32, u64, u64), u64>,
}
impl HistoryStore {
    pub fn ingest(&mut self, p: &[Process], dt: f64) {
        let mut seen = HashSet::new();
        for p in p.iter().filter(|p| p.raw.valid != 0) {
            let path = if p.info.app.is_empty() {
                &p.info.path
            } else {
                &p.info.app
            };
            let h = if let Some(entry) = self.entries.get_mut(path) {
                entry
            } else {
                self.entries.entry(path.clone()).or_insert_with(|| History {
                    name: p.info.title.clone(),
                    path: path.clone(),
                    ..Default::default()
                })
            };
            if let Some(old) = self.last.get(&p.raw.identity()) {
                h.cpu += p.raw.cpu_ns.saturating_sub(*old) as f64 / 1e9;
                h.disk += (p.read + p.write) * dt;
            }
            h.peak = h.peak.max(p.raw.memory);
            if seen.insert(path) {
                h.seconds += dt;
            }
        }
        self.last.clear();
        self.last.extend(
            p.iter()
                .filter(|p| p.raw.valid != 0)
                .map(|p| (p.raw.identity(), p.raw.cpu_ns)),
        );
        if self.entries.len() > 5000 {
            let mut values: Vec<_> = self.entries.drain().collect();
            values.sort_by(|a, b| b.1.cpu.total_cmp(&a.1.cpu));
            self.entries = values.into_iter().take(5000).collect();
        }
    }
    pub fn reset_baseline(&mut self) {
        self.last.clear()
    }
}
pub fn group_processes(p: &[Process]) -> Vec<(String, String, Vec<usize>)> {
    let lookup: HashMap<_, _> = p.iter().enumerate().map(|(i, p)| (p.raw.pid, i)).collect();
    let mut groups: HashMap<String, (String, Vec<usize>)> = HashMap::new();
    let mut visited = vec![usize::MAX; p.len()];
    for (i, proc) in p.iter().enumerate() {
        let mut root = proc;
        visited[i] = i;
        while root.info.app.is_empty() {
            if let Some(&n) = lookup.get(&root.raw.ppid) {
                if p[n].raw.pid <= 1 || visited[n] == i {
                    break;
                }
                visited[n] = i;
                root = &p[n];
            } else {
                break;
            }
        }
        let key = if root.info.app.is_empty() {
            format!("pid:{}", proc.id())
        } else {
            root.info.app.clone()
        };
        let title = if root.info.app.is_empty() {
            proc.info.name.clone()
        } else {
            root.info.title.clone()
        };
        groups.entry(key).or_insert((title, Vec::new())).1.push(i);
    }
    groups.into_iter().map(|(k, (n, v))| (k, n, v)).collect()
}
pub fn bytes(mut v: f64, binary: bool) -> String {
    let units = if binary {
        ["B", "KiB", "MiB", "GiB", "TiB"]
    } else {
        ["B", "KB", "MB", "GB", "TB"]
    };
    let base = if binary { 1024.0 } else { 1000.0 };
    let mut i = 0;
    while v >= base && i < 4 {
        v /= base;
        i += 1
    }
    if i == 0 {
        format!("{v:0.0} {}", units[i])
    } else {
        format!("{v:0.1} {}", units[i])
    }
}
pub fn duration(v: f64) -> String {
    let s = v.max(0.0) as u64;
    format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
}
pub fn push_history<T>(q: &mut VecDeque<T>, x: T, max: usize) {
    q.push_back(x);
    while q.len() > max {
        q.pop_front();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rates_handle_reset() {
        assert_eq!(rate(2, 9, 2.0), 0.0);
        assert_eq!(rate(20, 10, 2.0), 5.0);
        assert_eq!(rate(20, 10, 0.0), 0.0)
    }
    #[test]
    fn wrapping_core() {
        let p = Core {
            user: u32::MAX - 4,
            idle: 10,
            ..Default::default()
        };
        let n = Core {
            user: 5,
            idle: 20,
            ..Default::default()
        };
        assert_eq!(n.delta(p).unwrap().0, 50.0)
    }
    #[test]
    fn no_ticks_missing() {
        assert!(Core::default().delta(Core::default()).is_none())
    }
    #[test]
    fn idle_not_missing() {
        assert_eq!(
            Core {
                idle: 10,
                ..Default::default()
            }
            .delta(Core::default())
            .unwrap()
            .0,
            0.0
        )
    }
    #[test]
    fn history_bound() {
        let mut q = VecDeque::new();
        for i in 0..1000 {
            push_history(&mut q, i, 60)
        }
        assert_eq!(q.len(), 60);
        assert_eq!(q[0], 940)
    }
    #[test]
    fn graph_history_retains_five_minutes_at_high_speed_and_breaks_on_gaps() {
        let start = Instant::now();
        let mut history = VecDeque::new();
        let mut sample = Snapshot {
            elapsed: 0.25,
            ..Default::default()
        };
        for i in 0..=1204 {
            record_graph(
                &mut history,
                start + Duration::from_millis(i * 250),
                &sample,
            );
        }
        assert_eq!(history.len(), 1201);
        assert_eq!(history.front().unwrap().at, start + Duration::from_secs(1));
        assert!(history.back().unwrap().continuous);
        sample.elapsed = 0.5;
        record_graph(&mut history, start + Duration::from_millis(301500), &sample);
        assert!(history.back().unwrap().continuous);
        record_graph(&mut history, start + Duration::from_secs(305), &sample);
        assert!(!history.back().unwrap().continuous);
    }

    #[test]
    fn live_sampler() {
        let mut s = Sampler::default();
        let a = s.sample_with_threads(false);
        assert!(!a.processes.is_empty());
        assert!(a.system.raw.ram > 0);
        assert_eq!(a.system.cores.len(), a.system.raw.logical as usize);
        assert!(a
            .processes
            .iter()
            .find(|p| p.raw.pid == unsafe { libc::getpid() })
            .unwrap()
            .protected());
        let b = s.sample();
        let own_pid = unsafe { libc::getpid() };
        let first = a.processes.iter().find(|p| p.raw.pid == own_pid).unwrap();
        let second = b.processes.iter().find(|p| p.raw.pid == own_pid).unwrap();
        assert_eq!(first.raw.threads, -1);
        assert!(second.raw.threads > 0);
        assert_eq!(first.raw.valid, 1);
        assert!(first.raw.memory > 0);
        assert!(second.raw.cpu_ns >= first.raw.cpu_ns);
        assert!(Arc::ptr_eq(&first.info, &second.info));
        assert!(!first.info.path.is_empty());
        assert_ne!(first.raw.arch, 0);
        assert!(b.system.cpu.is_finite());
        assert!(b
            .system
            .cores
            .iter()
            .flatten()
            .all(|v| (0.0..=100.0).contains(&v.0)));
    }
    #[test]
    fn abi_layout() {
        assert_eq!(std::mem::size_of::<RawProcess>(), 4696);
        assert_eq!(std::mem::size_of::<Core>(), 16);
    }
}

#[cfg(test)]
mod regression_tests {
    use super::*;
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    fn process(pid: i32, ppid: i32, app: &str) -> Process {
        let mut raw: ProcessStats = unsafe { std::mem::zeroed() };
        raw.pid = pid;
        raw.ppid = ppid;
        raw.uid = unsafe { libc::getuid() } as i32;
        raw.valid = 1;
        raw.start = 123;
        raw.micro = 456;
        Process {
            raw,
            info: Arc::new(ProcessInfo {
                uuid: [0; 16],
                short_name: [0; 17],
                start_ticks: 0,
                arch: 0,
                name: format!("p{pid}"),
                path: format!("/tmp/p{pid}"),
                user: "tester".into(),
                app: app.into(),
                title: format!("p{pid}"),
                bundle: String::new(),
            }),
            cpu: 0.,
            read: 0.,
            write: 0.,
        }
    }
    #[test]
    fn grouping_cycles_terminate() {
        let p = vec![process(300, 301, ""), process(301, 300, "")];
        assert_eq!(group_processes(&p).len(), 2);
    }
    #[test]
    fn helper_inherits_bundle() {
        let p = vec![process(300, 1, "/tmp/App.app"), process(301, 300, "")];
        let g = group_processes(&p);
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].2.len(), 2);
    }
    #[test]
    fn identity_includes_microseconds() {
        let a = process(300, 1, "");
        let mut b = a.clone();
        b.raw.micro += 1;
        assert_ne!(a.id(), b.id());
    }
    #[test]
    fn foreign_and_system_processes_protected() {
        let mut a = process(300, 1, "");
        a.raw.uid += 1;
        assert!(a.protected());
        a.raw.uid = unsafe { libc::getuid() } as i32;
        Arc::make_mut(&mut a.info).path = "/System/Library/test".into();
        assert!(a.protected());
    }
    #[test]
    fn search_fields() {
        let mut a = process(300, 1, "");
        Arc::make_mut(&mut a.info).bundle = "org.sample.Test".into();
        assert!(a.matches("org.sample"));
        assert!(a.matches("300"));
        assert!(a.matches("tester"));
        assert!(!a.matches("nomatch"));
    }
    #[test]
    fn no_preobservation_history() {
        let mut store = HistoryStore::default();
        let mut a = process(300, 1, "");
        a.raw.cpu_ns = 2_000_000_000;
        store.ingest(&[a.clone()], 1.);
        assert_eq!(store.entries[&a.info.path].cpu, 0.);
        a.raw.cpu_ns += 500_000_000;
        store.ingest(&[a.clone()], 1.);
        assert_eq!(store.entries[&a.info.path].cpu, 0.5);
        store.reset_baseline();
        a.raw.cpu_ns += 3_000_000_000;
        store.ingest(&[a.clone()], 1.);
        assert_eq!(store.entries[&a.info.path].cpu, 0.5);
    }
    #[test]
    fn cached_metadata_changes_when_a_process_execs() {
        let mut child = Command::new("/bin/sh")
            .args(["-c", "read line; exec /bin/sleep 10"])
            .stdin(Stdio::piped())
            .spawn()
            .unwrap();
        let pid = child.id() as i32;
        let mut sampler = Sampler::default();
        let before = sampler
            .sample_with_threads(false)
            .processes
            .into_iter()
            .find(|p| p.raw.pid == pid)
            .unwrap();
        child.stdin.take().unwrap().write_all(b"run\n").unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        let after = loop {
            let current = sampler
                .sample_with_threads(false)
                .processes
                .into_iter()
                .find(|p| p.raw.pid == pid)
                .unwrap();
            if current.info.path.ends_with("/sleep") {
                break current;
            }
            assert!(
                Instant::now() < deadline,
                "cached executable did not change"
            );
            std::thread::sleep(Duration::from_millis(10));
        };
        child.kill().unwrap();
        child.wait().unwrap();
        assert_eq!(before.raw.identity(), after.raw.identity());
        assert!(!Arc::ptr_eq(&before.info, &after.info));
    }

    #[test]
    fn disposable_worker_identity_and_termination() {
        let mut child = std::process::Command::new("/bin/sleep")
            .arg("30")
            .spawn()
            .unwrap();
        let pid = child.id() as i32;
        let mut sampler = Sampler::default();
        let sample = sampler.sample();
        let p = sample
            .processes
            .into_iter()
            .find(|p| p.raw.pid == pid)
            .unwrap();
        let mut stale = p.clone();
        stale.raw.micro += 1;
        assert!(stale.revalidate().is_err());
        assert!(!p.protected());
        p.signal(libc::SIGTERM).unwrap();
        let status = child.wait().unwrap();
        assert!(!status.success());
    }
}
