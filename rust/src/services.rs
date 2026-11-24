use crate::startup::{Startup, StartupKind};
use std::{
    collections::{HashMap, HashSet},
    io::Read,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
#[derive(Clone, Debug, Default, PartialEq)]
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
            "enable" | "disable" => self.startup.is_some() && self.disabled == (action == "enable"),
            "start" | "stop" | "restart" => self.mutable,
            _ => false,
        }
    }

    pub fn startup_status(&self) -> &'static str {
        if self.disabled {
            "Disabled"
        } else if self.startup.as_ref().is_some_and(|s| s.approval_required) {
            "Needs approval"
        } else if self.startup.as_ref().is_some_and(Startup::bundled) {
            "Allowed"
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
        ("/System/Library/LaunchAgents", &*domain),
        ("/System/Library/LaunchDaemons", "system"),
        (&*local, &*domain),
        ("/Library/LaunchAgents", &*domain),
        ("/Library/LaunchDaemons", "system"),
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
            let agent =
                dom == domain && !folder.starts_with("/System/") && third_party_label(label);
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
                } else if folder == "/Library/LaunchDaemons" && third_party_label(label) {
                    Some(Startup::new(label, StartupKind::LaunchDaemon))
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
pub fn third_party_label(label: &str) -> bool {
    !label.trim().is_empty()
        && !label.starts_with("com.apple.")
        && !label.contains('/')
        && !label.chars().any(char::is_control)
}

#[derive(Default)]
struct StartupChanges<'a> {
    system: Vec<&'a Service>,
    user: Vec<&'a Service>,
    login: Vec<&'a Service>,
}

fn plan_changes<'a>(
    selected: &[Service],
    current: &'a [Service],
    gui_domain: &str,
) -> Result<StartupChanges<'a>, String> {
    let mut plan = StartupChanges::default();
    for item in selected {
        let fresh = current
            .iter()
            .find(|s| s.domain == item.domain && s.label == item.label)
            .ok_or_else(|| format!("{} is no longer available.", item.label))?;
        let startup = fresh
            .startup
            .as_ref()
            .ok_or("This item is no longer a startup item.")?;
        if item.startup.as_ref().map(|s| s.kind) != Some(startup.kind)
            || fresh.path.is_empty()
            || fresh.path != item.path
            || fresh.program != item.program
        {
            return Err(format!(
                "The configuration for {} has changed. Refresh and try again.",
                item.label
            ));
        }
        match startup.kind {
            StartupKind::LoginItem if fresh.domain == "login" && fresh.label == fresh.path => {
                plan.login.push(fresh)
            }
            _ if !third_party_label(&fresh.label) => {
                return Err("Invalid launchd helper label.".into())
            }
            StartupKind::BundledDaemon | StartupKind::LaunchDaemon if fresh.domain == "system" => {
                plan.system.push(fresh)
            }
            StartupKind::LaunchAgent
            | StartupKind::BundledAgent
            | StartupKind::BundledLoginItem
                if fresh.domain == gui_domain =>
            {
                plan.user.push(fresh)
            }
            _ => return Err("This startup item belongs to a different launchd domain.".into()),
        }
    }
    Ok(plan)
}

const SYSTEM_STARTUP_SCRIPT: &str = r#"on run argv
    set verb to item 1 of argv
    if verb is not in {"disable", "enable"} then error "Unknown startup action."
    set commands to "set -e"
    repeat with target in rest of argv
        set commands to commands & linefeed & "/bin/launchctl " & verb & " " & quoted form of (contents of target)
    end repeat
    try
        do shell script commands with administrator privileges with prompt "Task Manager wants to change startup for the selected helpers on this Mac."
    on error number -128
        return "cancelled"
    end try
    return "applied"
end run"#;

fn write_system_policy(action: &str, targets: &[String]) -> Result<bool, String> {
    let mut args = vec!["-e", SYSTEM_STARTUP_SCRIPT, action];
    args.extend(targets.iter().map(String::as_str));
    let output = command("/usr/bin/osascript", &args, Duration::from_secs(120))?;
    match output.trim() {
        "applied" => Ok(true),
        "cancelled" => Ok(false),
        _ => Err("macOS did not confirm the administrator request.".into()),
    }
}

fn apply_policy(
    items: &[&Service],
    disable: bool,
    read: impl Fn() -> Result<HashMap<String, bool>, String>,
    write: impl FnOnce(&[String]) -> Result<bool, String>,
) -> Result<bool, String> {
    if items.is_empty() {
        return Ok(true);
    }
    let before = read()?;
    let pending: Vec<_> = items
        .iter()
        .filter(|s| before.get(&s.label).copied().unwrap_or(s.disabled) != disable)
        .collect();
    if pending.is_empty() {
        return Ok(true);
    }
    let targets = pending.iter().map(|s| s.id()).collect::<Vec<_>>();
    if !write(&targets)? {
        return Ok(false);
    }
    let after = read()?;
    for s in pending {
        if after.get(&s.label) != Some(&disable) {
            return Err(format!(
                "launchd did not confirm the change to {}.",
                s.label
            ));
        }
    }
    Ok(true)
}

pub fn change_startup(selected: &[Service], disable: bool) -> Result<String, String> {
    let action = if disable { "disable" } else { "enable" };
    let gui_domain = format!("gui/{}", unsafe { libc::getuid() });
    let current = scan(true);
    let plan = plan_changes(selected, &current, &gui_domain)?;
    let policies = |domain: &str| launch(&["print-disabled", domain]).map(|s| disabled(&s));
    if !apply_policy(
        &plan.system,
        disable,
        || policies("system"),
        |targets| write_system_policy(action, targets),
    )? {
        return Ok(String::new());
    }
    apply_policy(
        &plan.user,
        disable,
        || policies(&gui_domain),
        |targets| {
            for target in targets {
                launch(&[action, target])?;
            }
            Ok(true)
        },
    )?;
    for item in plan.login {
        if item.disabled != disable {
            crate::startup::change_login_item(&item.path, action)?;
        }
    }
    Ok(String::new())
}

pub fn change_service(s: &Service, action: &str) -> Result<String, String> {
    let current = scan(false)
        .into_iter()
        .find(|n| n.id() == s.id())
        .ok_or("The service is no longer available.")?;
    if current.path != s.path {
        return Err("The service configuration has changed. Refresh and try again.".into());
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
    fn helper(label: &str, domain: &str, kind: StartupKind) -> Service {
        Service {
            label: label.into(),
            domain: domain.into(),
            path: format!("/Example.app/Contents/Library/{label}.plist"),
            startup: Some(Startup::new(label, kind)),
            ..Service::default()
        }
    }

    #[test]
    fn changes_use_the_confirmed_helpers_in_their_own_domains() {
        let gui = "gui/501";
        let agent = helper("org.example.agent", gui, StartupKind::BundledAgent);
        let daemon = helper("org.example.daemon", "system", StartupKind::BundledDaemon);
        let login = helper("org.example.login", gui, StartupKind::BundledLoginItem);
        let selected = [agent.clone(), daemon.clone(), login.clone()];
        let inventory = [login.clone(), daemon.clone(), agent.clone()];
        let plan = plan_changes(&selected, &inventory, gui).unwrap();
        assert_eq!(plan.system, [&daemon]);
        assert_eq!(plan.user, [&agent, &login]);
        let mut replaced = inventory;
        replaced[1].program = "/Different.app".into();
        assert!(plan_changes(&selected, &replaced, gui).is_err());
        assert!(plan_changes(&selected, &replaced[..1], gui).is_err());
        for invalid in [
            helper("com.apple.protected", "system", StartupKind::LaunchDaemon),
            helper("../other/job", "system", StartupKind::BundledDaemon),
            helper("org.example.agent", "system", StartupKind::BundledAgent),
            helper("org.example.agent", "gui/502", StartupKind::LaunchAgent),
        ] {
            let items = std::slice::from_ref(&invalid);
            assert!(plan_changes(items, items, gui).is_err());
        }
    }

    #[test]
    fn one_request_covers_the_group_and_a_repeat_skips_authorization() {
        let first = helper("org.example.a", "system", StartupKind::LaunchDaemon);
        let second = helper("org.example.b", "system", StartupKind::BundledDaemon);
        let policies = std::cell::RefCell::new(HashMap::new());
        let read = || Ok(policies.borrow().clone());
        assert!(apply_policy(&[&first, &second], true, read, |targets| {
            assert_eq!(targets, [first.id(), second.id()]);
            policies.replace(HashMap::from([
                (first.label.clone(), true),
                (second.label.clone(), true),
            ]));
            Ok(true)
        })
        .unwrap());
        assert!(apply_policy(&[&first, &second], true, read, |_| panic!(
            "already disabled"
        ))
        .unwrap());
    }

    #[test]
    fn retry_after_partial_failure_only_writes_remaining_helpers() {
        let a = helper("org.example.a", "system", StartupKind::LaunchDaemon);
        let b = helper("org.example.b", "system", StartupKind::LaunchDaemon);
        let policies = std::cell::RefCell::new(HashMap::new());
        let read = || Ok(policies.borrow().clone());
        let items = [&a, &b];
        assert!(apply_policy(&items, true, read, |_| {
            policies.borrow_mut().insert(a.label.clone(), true);
            Err("change denied".into())
        })
        .is_err());
        assert!(apply_policy(&items, true, read, |targets| {
            assert_eq!(targets, [b.id()]);
            policies.borrow_mut().insert(b.label.clone(), true);
            Ok(true)
        })
        .unwrap());
    }

    #[test]
    fn cancellation_denial_and_unconfirmed_changes_do_not_retry() {
        let item = helper("org.example.a", "system", StartupKind::LaunchDaemon);
        let read = || Ok(HashMap::new());
        assert!(!apply_policy(&[&item], true, read, |_| Ok(false)).unwrap());
        assert_eq!(
            apply_policy(&[&item], true, read, |_| Err("denied".into())),
            Err("denied".into())
        );
        assert!(apply_policy(&[&item], true, read, |_| Ok(true))
            .unwrap_err()
            .contains(&item.label));
        assert!(apply_policy(
            &[&item],
            true,
            || Err("read failed".into()),
            |_| panic!("missing policy")
        )
        .is_err());
    }

    #[test]
    fn background_approval_and_launchd_policy_are_independent() {
        let mut item = helper("org.example.a", "gui/501", StartupKind::LaunchAgent);
        item.startup.as_mut().unwrap().approval_required = true;
        assert_eq!(item.startup_status(), "Needs approval");
        assert!(item.permits("disable"));
        item.disabled = true;
        assert_eq!(item.startup_status(), "Disabled");
        assert!(item.permits("enable"));
    }

    #[test]
    fn timeout() {
        assert!(command("/bin/sleep", &["2"], Duration::from_millis(30)).is_err())
    }
}
