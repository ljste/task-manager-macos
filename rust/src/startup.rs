use crate::services::Service;
use objc2::rc::{autoreleasepool, Retained};
use objc2_foundation::{NSDictionary, NSNumber, NSString};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

extern "C" {
    fn tm_copy_login_items() -> *mut NSDictionary<NSString, NSNumber>;
    fn tm_legacy_requires_approval(path: &NSString) -> bool;
    fn tm_set_login_item(path: &NSString, enable: bool) -> *mut NSString;
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum StartupKind {
    LoginItem,
    LaunchAgent,
    Settings(&'static str),
    Bundled(&'static str),
}

#[derive(Clone)]
pub struct Startup {
    pub name: String,
    pub kind: StartupKind,
}

impl Startup {
    pub fn new(name: &str, kind: StartupKind) -> Self {
        Self {
            name: name.into(),
            kind,
        }
    }

    pub fn description(&self) -> &'static str {
        match self.kind {
            StartupKind::LoginItem => "Open at Login",
            StartupKind::LaunchAgent => "Launch agent",
            StartupKind::Settings(kind) | StartupKind::Bundled(kind) => kind,
        }
    }

    pub fn can_toggle(&self) -> bool {
        matches!(self.kind, StartupKind::LoginItem | StartupKind::LaunchAgent)
    }
}

pub fn change_login_item(path: &str, action: &str) -> Result<String, String> {
    let enable = match action {
        "enable" => true,
        "disable" => false,
        _ => return Err("Unsupported login item operation.".into()),
    };
    autoreleasepool(|_| {
        match unsafe { Retained::from_raw(tm_set_login_item(&NSString::from_str(path), enable)) } {
            Some(error) => Err(error.to_string()),
            None => Ok(String::new()),
        }
    })
}

fn login_items() -> HashMap<String, bool> {
    autoreleasepool(|_| {
        let mut result = HashMap::new();
        if let Some(items) = unsafe { Retained::from_raw(tm_copy_login_items()) } {
            for path in items.allKeys().iter() {
                if let Some(enabled) = items.objectForKey(&path) {
                    result.insert(path.to_string(), enabled.boolValue());
                }
            }
        }
        result
    })
}

pub fn requires_approval(path: &str) -> bool {
    autoreleasepool(|_| unsafe { tm_legacy_requires_approval(&NSString::from_str(path)) })
}

fn append_login_items(services: &mut Vec<Service>, login: HashMap<String, bool>) {
    for (path, enabled) in login {
        let name = Path::new(&path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(&path);
        services.push(Service {
            startup: Some(Startup::new(name, StartupKind::LoginItem)),
            label: path.clone(),
            program: path.clone(),
            path,
            domain: "login".into(),
            disabled: !enabled,
            ..Service::default()
        });
    }
}

fn entries(folder: &Path) -> impl Iterator<Item = PathBuf> {
    std::fs::read_dir(folder)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
}

fn app_bundles(folder: &Path) -> Vec<PathBuf> {
    let is_app = |p: &PathBuf| p.extension().is_some_and(|ext| ext == "app");
    let mut apps = Vec::new();
    for path in entries(folder) {
        if is_app(&path) {
            apps.push(path);
        } else if path.extension().is_none() && path.is_dir() {
            apps.extend(entries(&path).filter(is_app));
        }
    }
    apps
}

fn plist_string(file: &Path, key: &str) -> Option<String> {
    plist::Value::from_file(file)
        .ok()?
        .as_dictionary()?
        .get(key)?
        .as_string()
        .map(str::to_owned)
}

fn add_bundle_helpers(
    services: &mut Vec<Service>,
    apps: &[PathBuf],
    domain: &str,
    overrides: &HashMap<String, bool>,
) {
    let mut by_id: HashMap<_, _> = services
        .iter()
        .enumerate()
        .map(|(i, s)| (s.id(), i))
        .collect();
    for app in apps {
        let library = app.join("Contents/Library");
        let owner = app.file_stem().unwrap_or_default().to_string_lossy();
        for (folder, kind, job_domain) in [
            ("LoginItems", "Login helper", domain),
            ("LaunchAgents", "Bundled launch agent", domain),
            ("LaunchDaemons", "Bundled launch daemon", "system"),
        ] {
            for path in entries(&library.join(folder)) {
                let label = match path.extension().and_then(|p| p.to_str()) {
                    Some("app") if folder == "LoginItems" => {
                        plist_string(&path.join("Contents/Info.plist"), "CFBundleIdentifier")
                    }
                    Some("plist") if folder != "LoginItems" => plist_string(&path, "Label"),
                    _ => None,
                };
                let Some(label) = label.filter(|s| !s.is_empty() && !s.starts_with("com.apple."))
                else {
                    continue;
                };
                let id = format!("{job_domain}/{label}");
                let i = if let Some(&i) = by_id.get(&id) {
                    if !services[i].path.is_empty() {
                        continue;
                    }
                    i
                } else if let Some(disabled) = overrides.get(&id) {
                    let i = services.len();
                    by_id.insert(id, i);
                    services.push(Service {
                        label: label.clone(),
                        domain: job_domain.into(),
                        disabled: *disabled,
                        ..Service::default()
                    });
                    i
                } else {
                    continue;
                };
                let service = &mut services[i];
                let name = format!("{owner} ({label})");
                service.startup = Some(Startup::new(&name, StartupKind::Bundled(kind)));
                service.path = path.to_string_lossy().into_owned();
                service.program = app.to_string_lossy().into_owned();
            }
        }
    }
}

pub fn discover(
    services: &mut Vec<Service>,
    domain: &str,
    overrides: &HashMap<String, bool>,
    home: &Path,
) {
    autoreleasepool(|_| {
        for service in services.iter_mut().filter(|s| s.startup.is_some()) {
            if requires_approval(&service.path) {
                service.disabled = true;
                let kind = if service.domain == "system" {
                    "Launch daemon"
                } else {
                    "Launch agent"
                };
                service.startup = Some(Startup::new(&service.label, StartupKind::Settings(kind)));
            }
        }
        let login = login_items();
        let mut apps = app_bundles(Path::new("/Applications"));
        apps.extend(app_bundles(&home.join("Applications")));
        apps.extend(
            login
                .keys()
                .map(PathBuf::from)
                .filter(|p| p.extension().is_some_and(|ext| ext == "app")),
        );
        apps.sort();
        apps.dedup();
        add_bundle_helpers(services, &apps, domain, overrides);
        append_login_items(services, login);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_helpers_require_a_job_or_override_and_stay_system_managed() {
        let root =
            std::env::temp_dir().join(format!("task-manager-startup-test-{}", std::process::id()));
        let app = root.join("Example.app");
        let folder = app.join("Contents/Library/LaunchAgents");
        std::fs::create_dir_all(&folder).unwrap();
        let config = folder.join("example.plist");
        std::fs::write(&config, "<plist version=\"1.0\"><dict><key>Label</key><string>org.example.agent</string></dict></plist>").unwrap();
        let domain = "gui/501";
        let mut services = Vec::new();
        add_bundle_helpers(
            &mut services,
            std::slice::from_ref(&app),
            domain,
            &HashMap::new(),
        );
        assert!(services.is_empty());
        for (disabled, expected) in [(true, "Disabled"), (false, "See Settings")] {
            services.clear();
            let overrides = HashMap::from([(format!("{domain}/org.example.agent"), disabled)]);
            add_bundle_helpers(
                &mut services,
                std::slice::from_ref(&app),
                domain,
                &overrides,
            );
            add_bundle_helpers(
                &mut services,
                std::slice::from_ref(&app),
                domain,
                &overrides,
            );
            assert_eq!(services.len(), 1);
            assert_eq!(services[0].startup_status(), expected);
            assert!(!services[0].permits("enable") && !services[0].permits("disable"));
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn disabled_login_item_keeps_its_identity_and_enable_action() {
        let path = "/Applications/Example.app";
        let mut enabled = vec![];
        let mut disabled = vec![];
        append_login_items(&mut enabled, HashMap::from([(path.into(), true)]));
        append_login_items(&mut disabled, HashMap::from([(path.into(), false)]));
        assert_eq!(enabled[0].id(), disabled[0].id());
        assert!(enabled[0].permits("disable"));
        assert!(disabled[0].permits("enable"));
        assert_eq!(disabled[0].startup.as_ref().unwrap().name, "Example");
        assert!(!enabled[0].permits("stop"));
    }
}
