use crate::startup::{Startup, StartupKind};
use std::{
    collections::{HashMap, HashSet},
    io::Read,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
#[derive(Clone, Default)]
pub struct Service {
    pub label: String,
    pub program: String,
    pub path: String,
    pub domain: String,
    pub publisher: String,
    pub pid: Option<i32>,
    pub disabled: bool,
    pub mutable: bool,
    pub loaded: bool,
    pub startup: Option<Startup>,
}
impl Service {
    pub fn permits(&self, action: &str) -> bool {
        match action {
            "enable" | "disable" => {
                self.startup.as_ref().is_some_and(Startup::can_toggle)
                    && self.disabled == (action == "enable")
            }
            "start" | "stop" | "restart" => self.mutable,
            _ => false,
        }
    }

    pub fn startup_status(&self) -> &'static str {
        if self.disabled {
            "Disabled"
        } else if self
            .startup
            .as_ref()
            .is_some_and(|s| matches!(s.kind, StartupKind::Bundled(_)))
        {
            "See Settings"
        } else {
            "Enabled"
        }
    }

    pub fn id(&self) -> String {
        format!("{}/{}", self.domain, self.label)
    }
    pub fn status(&self) -> &str {
        if self.pid.is_some() {
            "Running"
        } else {
            "Stopped"
        }
    }
}
/// Drain concurrently, cap output, and reap every subprocess including timeouts.
pub fn command(program: &str, args: &[&str], timeout: Duration) -> Result<String, String> {
    let mut child = Command::new(program)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    let out = child.stdout.take().unwrap();
    let err = child.stderr.take().unwrap();
    let a = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = out.take(8 * 1024 * 1024).read_to_string(&mut s);
        s
    });
    let b = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = err.take(1024 * 1024).read_to_string(&mut s);
        s
    });
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break Some(s),
            Ok(None) if start.elapsed() < timeout => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    let output = a.join().unwrap_or_default();
    let errors = b.join().unwrap_or_default();
    match status {
        Some(s) if s.success() => Ok(if program == "/usr/bin/codesign" {
            format!("{output}{errors}")
        } else {
            output
        }),
        Some(status) => Err(if errors.trim().is_empty() {
            format!("{program} exited with {status}: {}", output.trim())
        } else {
            errors.trim().into()
        }),
        None => Err("The command timed out.".into()),
    }
}
pub fn launch(args: &[&str]) -> Result<String, String> {
    command("/bin/launchctl", args, Duration::from_secs(5))
}
pub fn jobs(text: &str) -> HashMap<String, Option<i32>> {
    let dump = text.contains("services = {");
    let mut inside = !dump;
    let mut r = HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if dump && line == "services = {" {
            inside = true;
            continue;
        }
        if dump && inside && line == "}" {
            break;
        }
        if !inside {
            continue;
        }
        let p: Vec<_> = line.split_whitespace().collect();
        if p.len() == 3
            && (p[0] == "-" || p[0].parse::<i32>().is_ok())
            && (p[1] == "-" || p[1] == "(pe)" || p[1].parse::<i32>().is_ok())
        {
            r.insert(p[2].into(), p[0].parse::<i32>().ok().filter(|n| *n > 0));
        }
    }
    r
}
pub fn disabled(text: &str) -> HashMap<String, bool> {
    text.lines()
        .filter_map(|l| {
            let (k, v) = l.split_once("=>")?;
            let disabled = match v.trim() {
                "true" | "disabled" => true,
                "false" | "enabled" => false,
                _ => return None,
            };
            Some((k.trim().trim_matches('"').into(), disabled))
        })
        .collect()
}
pub fn scan(startup: bool) -> Vec<Service> {
    let domain = format!("gui/{}", unsafe { libc::getuid() });
    let user = jobs(&launch(&["list"]).unwrap_or_default());
    let system = jobs(&launch(&["print", "system"]).unwrap_or_default());
    let ud = disabled(&launch(&["print-disabled", &domain]).unwrap_or_default());
    let sd = disabled(&launch(&["print-disabled", "system"]).unwrap_or_default());
    let home = std::env::var("HOME").unwrap_or_default();
    let local = format!("{home}/Library/LaunchAgents");
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    for (folder, dom) in [
        (&*local, &*domain),
        ("/Library/LaunchAgents", &*domain),
        ("/Library/LaunchDaemons", "system"),
        ("/System/Library/LaunchAgents", &*domain),
        ("/System/Library/LaunchDaemons", "system"),
    ] {
        let Ok(files) = std::fs::read_dir(folder) else {
            continue;
        };
        for f in files.flatten() {
            let path = f.path();
            if path.extension().and_then(|s| s.to_str()) != Some("plist") {
                continue;
            }
            let Ok(v) = plist::Value::from_file(&path) else {
                continue;
            };
            let Some(d) = v.as_dictionary() else { continue };
            let Some(label) = d.get("Label").and_then(|v| v.as_string()) else {
                continue;
            };
            let program = d
                .get("Program")
                .and_then(|v| v.as_string())
                .or_else(|| {
                    d.get("ProgramArguments")
                        .and_then(|v| v.as_array())
                        .and_then(|a| a.first())
                        .and_then(|v| v.as_string())
                })
                .unwrap_or("–");
            let j = if dom == "system" { &system } else { &user };
            let ov = if dom == "system" { &sd } else { &ud };
            let agent = dom == domain
                && !folder.starts_with("/System/")
                && !label.is_empty()
                && !label.starts_with("com.apple.")
                && !label.contains('/');
            let mutable = agent
                && folder == local
                && path
                    .canonicalize()
                    .ok()
                    .is_some_and(|p| p.starts_with(&local));
            let s = Service {
                label: label.into(),
                program: program.into(),
                path: path.to_string_lossy().into_owned(),
                domain: dom.into(),
                publisher: if label.starts_with("com.apple.") {
                    "Apple".into()
                } else {
                    label.split('.').nth(1).unwrap_or("Unknown").into()
                },
                pid: j.get(label).copied().flatten(),
                loaded: j.contains_key(label),
                disabled: ov.get(label).copied().unwrap_or_else(|| {
                    d.get("Disabled")
                        .and_then(|v| v.as_boolean())
                        .unwrap_or(false)
                }),
                mutable,
                startup: if agent {
                    Some(Startup::new(label, StartupKind::LaunchAgent))
                } else if folder == "/Library/LaunchDaemons" && !label.starts_with("com.apple.") {
                    Some(Startup::new(label, StartupKind::Settings("Launch daemon")))
                } else {
                    None
                },
            };
            if seen.insert(s.id()) {
                result.push(s);
            }
        }
    }
    for (dom, j, ov) in [(&*domain, &user, &ud), ("system", &system, &sd)] {
        for (label, pid) in j {
            let s = Service {
                label: label.clone(),
                domain: dom.into(),
                pid: *pid,
                disabled: ov.get(label).copied().unwrap_or(false),
                loaded: true,
                ..Default::default()
            };
            if seen.insert(s.id()) {
                result.push(s)
            }
        }
    }
    if startup {
        let overrides = [(&*domain, ud), ("system", sd)]
            .into_iter()
            .flat_map(|(domain, values)| {
                values
                    .into_iter()
                    .map(move |(label, disabled)| (format!("{domain}/{label}"), disabled))
            })
            .collect();
        crate::startup::discover(
            &mut result,
            &domain,
            &overrides,
            std::path::Path::new(&home),
        );
    }
    result.sort_by(|a, b| a.label.to_lowercase().cmp(&b.label.to_lowercase()));
    result
}
pub fn change(s: &Service, action: &str) -> Result<String, String> {
    if s.startup
        .as_ref()
        .is_some_and(|i| i.kind == StartupKind::LoginItem)
    {
        return crate::startup::change_login_item(&s.path, action);
    }
    let current = scan(false)
        .into_iter()
        .find(|n| n.id() == s.id())
        .ok_or("The service is no longer available.")?;
    if current.path != s.path {
        return Err("The service configuration has changed. Refresh and try again.".into());
    }
    if matches!(action, "enable" | "disable")
        && current
            .startup
            .as_ref()
            .is_some_and(|i| i.kind == StartupKind::LaunchAgent)
        && s.startup
            .as_ref()
            .is_some_and(|i| i.kind == StartupKind::LaunchAgent)
    {
        if action == "enable" && crate::startup::requires_approval(&current.path) {
            return Err("Allow this item in System Settings > General > Login Items first.".into());
        }
        launch(&[action, &current.id()])?;
        let overrides = disabled(&launch(&["print-disabled", &current.domain])?);
        return if overrides.get(&current.label) == Some(&(action == "disable")) {
            Ok(String::new())
        } else {
            Err("launchd did not confirm the startup change.".into())
        };
    }
    if !current.mutable {
        return Err("Only your own non-system LaunchAgents can be changed.".into());
    }
    match action {
        "start" => {
            if current.loaded {
                launch(&["kickstart", &s.id()])
            } else {
                launch(&["bootstrap", &s.domain, &s.path])
            }
        }
        "stop" => launch(&["bootout", &s.id()]),
        "restart" => launch(&["kickstart", "-k", &s.id()]),
        _ => Err("Unsupported operation".into()),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn list_jobs() {
        let j = jobs("PID Status Label\n12 0 com.test\n- 0 com.idle\n");
        assert_eq!(j["com.test"], Some(12));
        assert_eq!(j["com.idle"], None)
    }
    #[test]
    fn dump_jobs() {
        let j = jobs("services = {\n 42 (pe) foo\n}\n12 0 other");
        assert!(j.contains_key("foo"));
        assert!(!j.contains_key("other"))
    }
    #[test]
    fn overrides() {
        let map = disabled("disabled services = {\n \"a\" => disabled\n \"b\" => enabled\n \"c\" => true\n \"d\" => false\n \"e\" => unknown\n}");
        assert_eq!(
            map,
            HashMap::from([
                ("a".into(), true),
                ("b".into(), false),
                ("c".into(), true),
                ("d".into(), false),
            ])
        );
    }
    #[test]
    fn timeout() {
        assert!(command("/bin/sleep", &["2"], Duration::from_millis(30)).is_err())
    }
}
