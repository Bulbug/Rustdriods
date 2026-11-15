//! The window. Three tabs (Setup, System Images, Devices) and one shared log panel.
//! Heavy work runs on background threads and reports back through a channel.
use crate::adb;
use crate::keymap::{self, Profile};
use crate::{hook, inject};
use crate::devices::{self, DeviceInfo, Preset};
use crate::images::{self, SystemImage};
use crate::launcher::{self, Gpu, LaunchOptions};
use crate::{java, paths, sdk, virt, Res};
use eframe::egui;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::Duration;

const GREEN: egui::Color32 = egui::Color32::from_rgb(26, 127, 55);
const RED: egui::Color32 = egui::Color32::from_rgb(198, 40, 40);
const ORANGE: egui::Color32 = egui::Color32::from_rgb(214, 128, 0);

const SETUP_TEXT: &str = "Setting up the Android SDK means you accept Google's Android SDK License Agreement:\n\
https://developer.android.com/studio/terms\n\n\
If Java 17+ is missing, a small Java runtime (Eclipse Temurin 17, ~50 MB) is downloaded too.\n\n\
Continue?";

#[derive(PartialEq, Clone, Copy)]
enum Tab {
    Setup,
    Images,
    Devices,
    Tools,
    Keymap,
}

/// What the user confirmed in a dialog.
enum Action {
    Setup,
    InstallImage(usize),
    DeleteImage(usize),
    DeleteDevice(String),
    StopAndQuit,
    UninstallApp(String),
    ClearState(String),
    DeleteProfile(String),
}

struct EnvStatus {
    virt: (bool, String),
    java: bool,
    ram_mb: Option<u64>,
}

struct Snapshot {
    env: Option<EnvStatus>, // only filled on a "full" check (it is the slow part)
    tools: bool,
    adb: bool,
    emulator: bool,
    installed: Vec<bool>,
    devices: Vec<DeviceInfo>,
}

/// Messages from background threads to the window.
enum Msg {
    Log(String),
    Progress(Option<f32>),
    Refreshed(Snapshot),
    Finished { full: bool },
    EmuExited { code: i32 },
    Adb { connected: bool, apps: Vec<String> },
    Screen(adb::Screen),
    MapReady(Option<MapRuntime>),
    MapToggle(bool),
}

/// The emulator that is currently running (only one at a time).
struct Running {
    name: String,
    pid: Arc<AtomicU32>,
    alive: Arc<AtomicBool>,
    stopping: bool,
}

/// Handle given to background threads so they can talk to the window.
#[derive(Clone)]
struct Reporter {
    tx: mpsc::Sender<Msg>,
    ctx: egui::Context,
}

impl Reporter {
    fn send(&self, m: Msg) {
        let _ = self.tx.send(m);
        self.ctx.request_repaint(); // wake the UI (it sleeps when idle)
    }
    fn logger(&self) -> crate::runner::Log {
        let r = self.clone();
        Arc::new(move |s| r.send(Msg::Log(s)))
    }
    fn progress(&self, p: Option<f32>) {
        self.send(Msg::Progress(p));
    }
}

/// Everything that runs while keymapping is on. Dropping it turns mapping off.
struct MapRuntime {
    _hook: hook::HookHandle,
    _inj: inject::Injector,
}

/// What was clicked on the Keymap tab this frame.
#[derive(Default)]
struct KmClicks {
    select: Option<String>,
    create: bool,
    delete: bool,
    capture: bool,
    toggle: bool,
    pause: bool,
    refresh: bool,
    clicked_at: Option<(f32, f32)>,
    remove: Option<usize>,
    test: Option<usize>,
    save: bool,
}

/// What was clicked on the ADB Tools tab this frame.
#[derive(Default)]
struct ToolClicks {
    install: bool,
    shot: bool,
    open_shots: bool,
    uninstall: bool,
    refresh: bool,
    system_changed: bool,
    run_shell: bool,
    push: bool,
    pull: bool,
    open_pulled: bool,
}

const SHELL_EXAMPLES: [(&str, &str); 6] = [
    ("Android version", "getprop ro.build.version.release"),
    ("Battery", "dumpsys battery"),
    ("Free space", "df -h /data"),
    ("Last 100 log lines", "logcat -d -t 100"),
    ("Press Home", "input keyevent 3"),
    ("Press Back", "input keyevent 4"),
];

pub struct App {
    rep: Reporter,
    rx: mpsc::Receiver<Msg>,

    tab: Tab,
    busy: bool,
    progress: Option<f32>,
    log: Vec<String>,

    virt: (bool, String),
    java_ok: bool,
    tools_ok: bool,
    adb_ok: bool,
    emu_ok: bool,

    images: Vec<SystemImage>,
    installed: Vec<bool>,
    sel_image: Option<usize>,

    devices: Vec<DeviceInfo>,
    sel_device: Option<usize>,
    new_name: String,
    new_image: Option<usize>,
    new_preset: Preset,

    adb_connected: bool,
    apps: Vec<String>,
    sel_app: Option<String>,
    app_filter: String,
    show_system: bool,
    apk_path: String,
    shell_cmd: String,
    push_src: String,
    push_dest: String,
    pull_src: String,

    pc_ram_mb: Option<u64>,
    pc_cores: usize,
    preset_auto_set: bool,
    edit_for: Option<String>,
    edit_ram: u32,
    edit_cores: u32,
    edit_preset: Preset,

    km_profiles: Vec<String>,
    km: Option<Profile>,
    km_new_name: String,
    km_tex: Option<egui::TextureHandle>,
    km_dev_size: Option<(u32, u32)>,
    km_pending: Option<(f32, f32)>,
    km_runtime: Option<MapRuntime>,
    km_active: bool,
    km_starting: bool,

    running: Option<Running>,
    opts: LaunchOptions,
    quit_when_stopped: bool,
    quitting: bool,

    confirm: Option<(String, Action)>,
    info: Option<String>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let (tx, rx) = mpsc::channel();
        let images = images::catalog();
        let app = App {
            rep: Reporter { tx, ctx: cc.egui_ctx.clone() },
            rx,
            tab: Tab::Setup,
            busy: false,
            progress: None,
            log: Vec::new(),
            virt: (false, "Checking...".to_string()),
            java_ok: false,
            tools_ok: false,
            adb_ok: false,
            emu_ok: false,
            installed: vec![false; images.len()],
            images,
            sel_image: None,
            devices: Vec::new(),
            sel_device: None,
            new_name: String::new(),
            new_image: None,
            new_preset: Preset::Lite,
            adb_connected: false,
            apps: Vec::new(),
            sel_app: None,
            app_filter: String::new(),
            show_system: false,
            apk_path: String::new(),
            shell_cmd: String::new(),
            push_src: String::new(),
            push_dest: "/sdcard/Download/".to_string(),
            pull_src: "/sdcard/Download/".to_string(),
            pc_ram_mb: None,
            pc_cores: thread::available_parallelism().map(|n| n.get()).unwrap_or(2),
            preset_auto_set: false,
            edit_for: None,
            edit_ram: 2048,
            edit_cores: 2,
            edit_preset: Preset::Lite,
            km_profiles: keymap::list_profiles(),
            km: None,
            km_new_name: String::new(),
            km_tex: None,
            km_dev_size: None,
            km_pending: None,
            km_runtime: None,
            km_active: false,
            km_starting: false,
            running: None,
            opts: LaunchOptions::default(),
            quit_when_stopped: false,
            quitting: false,
            confirm: None,
            info: None,
        };
        app.refresh(true);
        app
    }

    // ---------- background work ----------

    /// Re-read what is installed. `full` also re-checks virtualization and Java (slower).
    fn refresh(&self, full: bool) {
        let rep = self.rep.clone();
        let imgs = self.images.clone();
        thread::spawn(move || {
            let env = if full {
                let v = virt::check();
                Some(EnvStatus {
                    virt: (v.ok, v.message),
                    java: java::java_home().is_some(),
                    ram_mb: v.ram_mb,
                })
            } else {
                None
            };
            let snap = Snapshot {
                env,
                tools: sdk::has_cmdline_tools(),
                adb: sdk::has_platform_tools(),
                emulator: sdk::has_emulator(),
                installed: imgs.iter().map(|i| i.is_installed()).collect(),
                devices: devices::list().unwrap_or_default(),
            };
            rep.send(Msg::Refreshed(snap));
        });
    }

    fn spawn_job<F>(&mut self, full: bool, job: F)
    where
        F: FnOnce(&Reporter) -> Res<()> + Send + 'static,
    {
        self.busy = true;
        self.progress = None;
        let rep = self.rep.clone();
        thread::spawn(move || {
            if let Err(e) = job(&rep) {
                rep.send(Msg::Log(format!("ERROR: {e}")));
            }
            rep.send(Msg::Finished { full });
        });
    }

    fn run_action(&mut self, action: Action) {
        match action {
            Action::Setup => self.spawn_job(true, |rep| {
                let log = rep.logger();
                if java::java_home().is_none() {
                    java::download_jre(&|p| rep.progress(Some(p)), &log)?;
                }
                if !sdk::has_cmdline_tools() {
                    sdk::download_cmdline_tools(&|p| rep.progress(Some(p)), &log)?;
                }
                rep.progress(None);
                sdk::install_packages(&log)?;
                log("Done!".to_string());
                Ok(())
            }),
            Action::InstallImage(i) => {
                let img = self.images[i].clone();
                self.spawn_job(false, move |rep| images::install(&img, &rep.logger()));
            }
            Action::DeleteImage(i) => {
                let img = self.images[i].clone();
                self.spawn_job(false, move |rep| images::uninstall(&img, &rep.logger()));
            }
            Action::DeleteDevice(name) => {
                self.spawn_job(false, move |rep| devices::delete(&name, &rep.logger()));
            }
            Action::StopAndQuit => {
                self.quit_when_stopped = true;
                self.on_stop_device();
            }
            Action::DeleteProfile(name) => {
                if let Err(e) = keymap::delete(&name) {
                    self.push_log(format!("ERROR: {e}"));
                }
                self.km_profiles = keymap::list_profiles();
                if self.km.as_ref().map_or(false, |p| p.name == name) {
                    self.km = None;
                }
            }
            Action::ClearState(name) => {
                self.spawn_job(false, move |rep| devices::clear_snapshots(&name, &rep.logger()));
            }
            Action::UninstallApp(pkg) => {
                let show_system = self.show_system;
                self.spawn_job(false, move |rep| {
                    adb::require_connected()?;
                    adb::uninstall(&pkg, &rep.logger())?;
                    let (connected, apps) = adb::snapshot(show_system);
                    rep.send(Msg::Adb { connected, apps });
                    Ok(())
                });
            }
        }
    }

    // ---------- messages from threads ----------

    fn drain(&mut self) {
        while let Ok(m) = self.rx.try_recv() {
            match m {
                Msg::Log(s) => self.push_log(s),
                Msg::Progress(p) => self.progress = p,
                Msg::Refreshed(s) => self.apply(s),
                Msg::Finished { full } => {
                    self.busy = false;
                    self.progress = None;
                    self.refresh(full);
                }
                Msg::Adb { connected, apps } => {
                    self.adb_connected = connected;
                    self.apps = apps;
                    if let Some(sel) = &self.sel_app {
                        if !self.apps.contains(sel) {
                            self.sel_app = None;
                        }
                    }
                }
                Msg::Screen(s) => {
                    let img = egui::ColorImage::from_rgba_unmultiplied([s.sw, s.sh], &s.rgba);
                    self.km_tex = Some(self.rep.ctx.load_texture("device_screen", img, egui::TextureOptions::LINEAR));
                    self.km_dev_size = Some((s.w, s.h));
                    let landscape = s.w > s.h;
                    if let Some(p) = self.km.as_mut() {
                        if p.binds.is_empty() {
                            p.landscape = landscape;
                        } else if p.landscape != landscape {
                            self.push_log("Warning: the screen is rotated differently from when this profile was made.".to_string());
                        }
                    }
                    self.push_log(format!("Captured the screen ({}x{}). Click where a button is, then press a key.", s.w, s.h));
                }
                Msg::MapReady(rt) => {
                    self.km_active = rt.is_some();
                    self.km_runtime = rt;
                    self.km_starting = false;
                }
                Msg::MapToggle(on) => self.km_active = on,
                Msg::EmuExited { code } => {
                    self.km_runtime = None; // the emulator is gone, so mapping stops too
                    self.km_active = false;
                    self.running = None;
                    self.adb_connected = false;
                    self.apps.clear();
                    self.sel_app = None;
                    self.push_log(format!("Emulator closed (exit code {code})."));
                    if self.quit_when_stopped {
                        self.quitting = true;
                        self.rep.ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                }
            }
        }
    }

    fn push_log(&mut self, s: String) {
        // sdkmanager prints a progress bar many times; keep only the latest one.
        let is_bar = |t: &str| t.starts_with('[') && t.contains('%');
        if is_bar(&s) {
            if let Some(last) = self.log.last_mut() {
                if is_bar(last) {
                    *last = s;
                    return;
                }
            }
        }
        self.log.push(s);
        if self.log.len() > 1000 {
            self.log.drain(..200);
        }
    }

    fn apply(&mut self, s: Snapshot) {
        if let Some(env) = s.env {
            self.virt = env.virt;
            self.java_ok = env.java;
            self.pc_ram_mb = env.ram_mb;
            if !self.preset_auto_set {
                if let Some(ram) = env.ram_mb {
                    self.new_preset = devices::recommend(ram, self.pc_cores);
                    self.preset_auto_set = true;
                }
            }
        }
        self.tools_ok = s.tools;
        self.adb_ok = s.adb;
        self.emu_ok = s.emulator;
        self.installed = s.installed;
        self.devices = s.devices;
        if self.sel_device.map_or(false, |i| i >= self.devices.len()) {
            self.sel_device = None;
        }
        let still_ok = self.new_image.map_or(false, |i| self.installed[i]);
        if !still_ok {
            self.new_image = self.installed.iter().position(|&b| b);
        }
    }

    // ---------- button handlers ----------

    fn on_install_image(&mut self) {
        let Some(i) = self.sel_image else {
            self.info = Some("Select an image first.".to_string());
            return;
        };
        if self.installed[i] {
            self.info = Some("That image is already installed.".to_string());
            return;
        }
        self.confirm = Some((
            format!("Download {}?\nThis can be 1-2 GB.", self.images[i].display_name()),
            Action::InstallImage(i),
        ));
    }

    fn on_delete_image(&mut self) {
        let Some(i) = self.sel_image else {
            self.info = Some("Select an image first.".to_string());
            return;
        };
        if !self.installed[i] {
            self.info = Some("That image is not installed.".to_string());
            return;
        }
        match devices::devices_using(&self.images[i]) {
            Ok(users) if !users.is_empty() => {
                self.info = Some(format!(
                    "Can't delete this image. Used by: {}.\nDelete those devices first (Devices tab).",
                    users.join(", ")
                ));
            }
            Ok(_) => {
                self.confirm = Some((
                    format!("Delete {}?", self.images[i].display_name()),
                    Action::DeleteImage(i),
                ));
            }
            Err(e) => self.push_log(format!("ERROR: {e}")),
        }
    }

    fn on_create_device(&mut self) {
        let name = self.new_name.trim().to_string();
        if name.is_empty() {
            self.info = Some("Type a name for the device.".to_string());
            return;
        }
        let Some(i) = self.new_image else {
            self.info = Some("Pick an installed image.".to_string());
            return;
        };
        let img = self.images[i].clone();
        let preset = self.new_preset;
        self.new_name.clear();
        self.spawn_job(false, move |rep| devices::create(&name, &img, preset, &rep.logger()));
    }

    fn on_delete_device(&mut self) {
        let Some(i) = self.sel_device else {
            self.info = Some("Select a device first.".to_string());
            return;
        };
        let name = self.devices[i].name.clone();
        if self.running.as_ref().map_or(false, |r| r.name == name) {
            self.info = Some("Stop this device before deleting it.".to_string());
            return;
        }
        self.confirm = Some((
            format!("Delete device \"{name}\"? Its data will be erased."),
            Action::DeleteDevice(name),
        ));
    }

    /// Re-check the emulator connection and the app list (on demand only, never in the background).
    fn adb_refresh(&self) {
        let rep = self.rep.clone();
        let show_system = self.show_system;
        thread::spawn(move || {
            let (connected, apps) = adb::snapshot(show_system);
            rep.send(Msg::Adb { connected, apps });
        });
    }

    fn on_install_apk(&mut self, path: PathBuf) {
        if self.busy {
            self.info = Some("Please wait for the current task to finish.".to_string());
            return;
        }
        if !self.adb_ok {
            self.info = Some("Set up the Android SDK first (Setup tab).".to_string());
            return;
        }
        let is_apk = path.extension().map_or(false, |e| e.eq_ignore_ascii_case("apk"));
        if !is_apk {
            self.info = Some("Only .apk files can be installed here (.xapk / .apks are not supported).".to_string());
            return;
        }
        if !path.is_file() {
            self.info = Some(format!("File not found: {}", path.display()));
            return;
        }
        let show_system = self.show_system;
        self.spawn_job(false, move |rep| {
            adb::require_connected()?;
            adb::install(&path, &rep.logger())?;
            let (connected, apps) = adb::snapshot(show_system);
            rep.send(Msg::Adb { connected, apps });
            Ok(())
        });
    }

    fn on_screenshot(&mut self) {
        self.spawn_job(false, |rep| {
            adb::require_connected()?;
            let path = adb::screenshot()?;
            rep.logger()(format!("Screenshot saved: {}", path.display()));
            Ok(())
        });
    }

    fn on_uninstall_app(&mut self) {
        let Some(pkg) = self.sel_app.clone() else {
            self.info = Some("Select an app first.".to_string());
            return;
        };
        self.confirm = Some((format!("Uninstall {pkg}?\nIts data will be erased."), Action::UninstallApp(pkg)));
    }

    fn on_save_resources(&mut self) {
        let Some(i) = self.sel_device else { return };
        let name = self.devices[i].name.clone();
        if self.running.as_ref().map_or(false, |r| r.name == name) {
            self.info = Some("Stop this device before changing it.".to_string());
            return;
        }
        let (ram, cores) = (self.edit_ram, self.edit_cores);
        self.spawn_job(false, move |rep| devices::set_resources(&name, ram, cores, &rep.logger()));
    }

    fn on_clear_state(&mut self) {
        let Some(i) = self.sel_device else { return };
        let name = self.devices[i].name.clone();
        if self.running.as_ref().map_or(false, |r| r.name == name) {
            self.info = Some("Stop this device before clearing its saved state.".to_string());
            return;
        }
        self.confirm = Some((
            format!("Clear the saved state of \"{name}\"?\nThe next start will be a full (slower) boot."),
            Action::ClearState(name),
        ));
    }

    // ---------- keymap ----------

    fn km_save(&mut self) {
        let res = self.km.as_ref().map(keymap::save);
        if let Some(Err(e)) = res {
            self.push_log(format!("ERROR: {e}"));
        }
    }

    fn km_select(&mut self, name: &str) {
        match keymap::load(name) {
            Ok(p) => self.km = Some(p),
            Err(e) => self.push_log(format!("ERROR: {e}")),
        }
    }

    fn km_create(&mut self) {
        let name = self.km_new_name.trim().to_string();
        if !keymap::valid_name(&name) {
            self.info = Some("Use a short name with letters, numbers, spaces, - _ .".to_string());
            return;
        }
        self.km_new_name.clear();
        if self.km_profiles.contains(&name) {
            self.km_select(&name);
            return;
        }
        let p = Profile {
            name,
            landscape: self.km_dev_size.map_or(false, |(w, h)| w > h),
            binds: Vec::new(),
        };
        if let Err(e) = keymap::save(&p) {
            self.push_log(format!("ERROR: {e}"));
            return;
        }
        self.km = Some(p);
        self.km_profiles = keymap::list_profiles();
    }

    fn km_capture(&mut self) {
        self.spawn_job(false, |rep| {
            adb::require_connected()?;
            let screen = adb::capture_small(960)?;
            rep.send(Msg::Screen(screen));
            Ok(())
        });
    }

    fn km_test(&mut self, i: usize) {
        let (Some(p), Some(size)) = (&self.km, self.km_dev_size) else { return };
        let Some(b) = p.binds.get(i) else { return };
        let (x, y) = inject::to_pixels(b.x, b.y, size);
        let key = b.key.clone();
        self.spawn_job(false, move |rep| {
            adb::require_connected()?;
            adb::tap(x, y)?;
            rep.logger()(format!("Tapped ({x}, {y}) for {key}."));
            Ok(())
        });
    }

    /// Turns mapping on or off.
    fn km_toggle(&mut self) {
        if self.km_runtime.is_some() {
            self.km_runtime = None; // dropping it removes the keyboard hook and stops the injector
            self.km_active = false;
            self.push_log("Keymapping is off.".to_string());
            return;
        }
        let Some(p) = &self.km else { return };
        let mut binds: HashMap<u32, (f32, f32)> = HashMap::new();
        for b in &p.binds {
            if let Some(vk) = keymap::vk_for_label(&b.key) {
                binds.insert(vk, (b.x, b.y));
            }
        }
        if binds.is_empty() {
            self.info = Some("This profile has no key bindings yet.".to_string());
            return;
        }
        let landscape = p.landscape;
        self.km_starting = true;

        let rep = self.rep.clone();
        thread::spawn(move || {
            let result = (|| -> Result<(MapRuntime, (u32, u32)), String> {
                adb::require_connected().map_err(|e| e.to_string())?;
                let (w, h) = adb::screen_size().map_err(|e| e.to_string())?;
                // The profile was made for one orientation; use the screen size in that orientation.
                let size = if landscape { (w.max(h), w.min(h)) } else { (w.min(h), w.max(h)) };
                let inj = inject::start(size);
                let rep2 = rep.clone();
                let hook = hook::start(
                    binds,
                    inj.tx.clone(),
                    Box::new(move |on| rep2.send(Msg::MapToggle(on))),
                )?;
                Ok((MapRuntime { _hook: hook, _inj: inj }, size))
            })();
            match result {
                Ok((rt, size)) => {
                    rep.send(Msg::Log(format!(
                        "Keymapping is ON ({}x{} screen). Click into the emulator window; {} pauses.",
                        size.0, size.1, keymap::TOGGLE_LABEL
                    )));
                    rep.send(Msg::MapReady(Some(rt)));
                }
                Err(e) => {
                    rep.send(Msg::Log(format!("ERROR: {e}")));
                    rep.send(Msg::MapReady(None));
                }
            }
        });
    }

    fn on_run_shell(&mut self) {
        let cmd = self.shell_cmd.trim().to_string();
        if cmd.is_empty() {
            self.info = Some("Type a command first (or click one of the examples).".to_string());
            return;
        }
        if self.busy || !self.adb_ok {
            return;
        }
        self.spawn_job(false, move |rep| {
            adb::require_connected()?;
            let log = rep.logger();
            log(format!("$ {cmd}"));
            adb::shell(&cmd, &log)
        });
    }

    fn on_push(&mut self) {
        let src = adb::clean_path(&self.push_src);
        let dest = adb::clean_path(&self.push_dest);
        if src.is_empty() {
            self.info = Some("Drop a file here, or paste the file's path first.".to_string());
            return;
        }
        let dest = if dest.is_empty() { "/sdcard/Download/".to_string() } else { dest };
        if self.busy || !self.adb_ok {
            return;
        }
        self.spawn_job(false, move |rep| {
            adb::require_connected()?;
            adb::push(&PathBuf::from(src), &dest, &rep.logger())
        });
    }

    fn on_pull(&mut self) {
        let src = adb::clean_path(&self.pull_src);
        if src.is_empty() {
            self.info = Some("Type the path of the file inside the emulator, e.g. /sdcard/Download/file.txt".to_string());
            return;
        }
        if self.busy || !self.adb_ok {
            return;
        }
        self.spawn_job(false, move |rep| {
            adb::require_connected()?;
            adb::pull(&src, &paths::pulled_dir(), &rep.logger())
        });
    }

    fn on_start_device(&mut self) {
        let Some(i) = self.sel_device else {
            self.info = Some("Select a device first.".to_string());
            return;
        };
        if self.running.is_some() {
            self.info = Some("An emulator is already running. Stop it first.".to_string());
            return;
        }
        if !self.emu_ok {
            self.info = Some("The Android Emulator is not installed. Run Setup first.".to_string());
            return;
        }
        let name = self.devices[i].name.clone();
        let opts = self.opts.clone();
        let pid = Arc::new(AtomicU32::new(0));
        let alive = Arc::new(AtomicBool::new(true));
        self.running = Some(Running {
            name: name.clone(),
            pid: Arc::clone(&pid),
            alive: Arc::clone(&alive),
            stopping: false,
        });

        // A watcher thread runs the emulator and waits for it to exit.
        let rep = self.rep.clone();
        thread::spawn(move || {
            let log = rep.logger();
            let code = match launcher::start(&name, &opts, &log, &|p| pid.store(p, Ordering::SeqCst)) {
                Ok(c) => c,
                Err(e) => {
                    rep.send(Msg::Log(format!("ERROR: {e}")));
                    -1
                }
            };
            alive.store(false, Ordering::SeqCst);
            rep.send(Msg::EmuExited { code });
        });
    }

    fn on_stop_device(&mut self) {
        let Some(r) = self.running.as_mut() else { return };
        if r.stopping {
            return;
        }
        r.stopping = true;
        let pid = Arc::clone(&r.pid);
        let alive = Arc::clone(&r.alive);
        let rep = self.rep.clone();
        thread::spawn(move || {
            let log = rep.logger();
            log("Stopping emulator...".to_string());
            let _ = launcher::stop_graceful(&log); // polite: adb emu kill
            for _ in 0..40 {
                if !alive.load(Ordering::SeqCst) {
                    return;
                }
                thread::sleep(Duration::from_millis(500));
            }
            log("The emulator did not close in time - forcing it.".to_string());
            launcher::kill_tree(pid.load(Ordering::SeqCst));
        });
    }

    // ---------- tabs ----------

    fn ui_setup(&mut self, ui: &mut egui::Ui) {
        ui.heading("Setup");
        ui.add_space(6.0);
        egui::Grid::new("status").num_columns(2).spacing([20.0, 8.0]).show(ui, |ui| {
            status_row(ui, "Virtualization", self.virt.0, &self.virt.1);
            let java_text = if self.java_ok {
                "Found"
            } else {
                "Not found - a small runtime (~50 MB) will be downloaded"
            };
            status_row(ui, "Java 17+ (for SDK tools)", self.java_ok, java_text);
            status_row(ui, "SDK command-line tools", self.tools_ok, yes_no(self.tools_ok));
            status_row(ui, "Platform-tools (adb)", self.adb_ok, yes_no(self.adb_ok));
            status_row(ui, "Android Emulator", self.emu_ok, yes_no(self.emu_ok));
        });
        ui.add_space(10.0);

        let all_ready = self.java_ok && self.tools_ok && self.adb_ok && self.emu_ok;
        let (mut setup, mut check) = (false, false);
        ui.horizontal(|ui| {
            setup = ui
                .add_enabled(!self.busy && !all_ready, egui::Button::new("Set up Android SDK"))
                .clicked();
            check = ui.add_enabled(!self.busy, egui::Button::new("Check again")).clicked();
        });
        if setup {
            self.confirm = Some((SETUP_TEXT.to_string(), Action::Setup));
        }
        if check {
            self.refresh(true);
        }

        // About: handy for bug reports, and shows where everything is stored.
        ui.add_space(16.0);
        ui.separator();
        ui.weak(format!("Android Emulator Manager v{}", env!("CARGO_PKG_VERSION")));
        ui.weak(format!("Data folder: {}", paths::home().display()));
        if ui.small_button("Open data folder").clicked() {
            adb::open_folder(&paths::home());
        }
    }

    fn ui_images(&mut self, ui: &mut egui::Ui) {
        ui.heading("System Images");
        if !self.tools_ok {
            ui.colored_label(RED, "Set up the Android SDK first (Setup tab).");
        }
        ui.add_space(6.0);

        let mut sel = self.sel_image;
        egui::Grid::new("images").striped(true).num_columns(3).spacing([28.0, 6.0]).show(ui, |ui| {
            ui.strong("Android");
            ui.strong("Type");
            ui.strong("Status");
            ui.end_row();
            for (i, img) in self.images.iter().enumerate() {
                if ui.selectable_label(sel == Some(i), img.android_label()).clicked() {
                    sel = Some(i);
                }
                ui.label(img.variant);
                if self.installed[i] {
                    ui.colored_label(GREEN, "Installed");
                } else {
                    ui.label("Not installed");
                }
                ui.end_row();
            }
        });
        self.sel_image = sel;

        ui.add_space(6.0);
        ui.label(
            "Tip: \"Google APIs\" allows root. \"Google Play\" has the Play Store but can't be rooted. \
             \"AOSP\" has no Google services and is the lightest.",
        );
        ui.add_space(8.0);

        let (mut install, mut delete, mut refresh) = (false, false, false);
        ui.horizontal(|ui| {
            install = ui
                .add_enabled(!self.busy && self.tools_ok, egui::Button::new("Install selected"))
                .clicked();
            delete = ui
                .add_enabled(!self.busy && self.tools_ok, egui::Button::new("Delete selected"))
                .clicked();
            refresh = ui.add_enabled(!self.busy, egui::Button::new("Refresh")).clicked();
        });
        if install {
            self.on_install_image();
        }
        if delete {
            self.on_delete_image();
        }
        if refresh {
            self.refresh(false);
        }
    }

    fn ui_devices(&mut self, ui: &mut egui::Ui) {
        ui.heading("Devices");
        let any_installed = self.installed.iter().any(|&b| b);
        if !self.tools_ok {
            ui.colored_label(RED, "Set up the Android SDK first (Setup tab).");
        } else if !any_installed {
            ui.colored_label(RED, "Install a system image first (System Images tab).");
        }
        ui.add_space(6.0);

        if self.devices.is_empty() {
            ui.label("No devices yet. Create one below.");
        } else {
            let mut sel = self.sel_device;
            egui::Grid::new("devices").striped(true).num_columns(7).spacing([24.0, 6.0]).show(ui, |ui| {
                for h in ["Name", "Android", "RAM", "Cores", "Screen", "Saved state", "Status"] {
                    ui.strong(h);
                }
                ui.end_row();
                for (i, d) in self.devices.iter().enumerate() {
                    if ui.selectable_label(sel == Some(i), &d.name).clicked() {
                        sel = Some(i);
                    }
                    ui.label(friendly(&self.images, &d.sysdir));
                    ui.label(devices::ram_label(&d.ram));
                    ui.label(&d.cores);
                    ui.label(&d.screen);
                    ui.label(devices::human_size(d.snap_bytes));
                    if self.running.as_ref().map_or(false, |r| r.name == d.name) {
                        ui.colored_label(GREEN, "Running");
                    } else {
                        ui.label("");
                    }
                    ui.end_row();
                }
            });
            self.sel_device = sel;
        }

        ui.add_space(8.0);
        let running_now = self.running.is_some();
        let stopping = self.running.as_ref().map_or(false, |r| r.stopping);
        let (mut start, mut stop) = (false, false);
        ui.horizontal(|ui| {
            start = ui
                .add_enabled(!running_now && !self.busy && self.tools_ok, egui::Button::new("Start selected"))
                .clicked();
            stop = ui.add_enabled(running_now && !stopping, egui::Button::new("Stop")).clicked();
            if let Some(r) = &self.running {
                ui.colored_label(GREEN, format!("Running: {}", r.name));
            }
        });
        egui::CollapsingHeader::new("Launch options (lean by default)").show(ui, |ui| {
            ui.checkbox(&mut self.opts.no_audio, "Mute audio (saves CPU)");
            ui.checkbox(&mut self.opts.no_boot_anim, "Skip boot animation");
            ui.checkbox(&mut self.opts.quick_boot, "Quick boot (resume from saved state)");
            ui.checkbox(&mut self.opts.no_save, "Don't save state on exit (saves disk space)");
            ui.horizontal(|ui| {
                ui.label("Graphics:");
                egui::ComboBox::from_id_salt("gpu").selected_text(self.opts.gpu.label()).show_ui(ui, |ui| {
                    for g in Gpu::ALL {
                        ui.selectable_value(&mut self.opts.gpu, g, g.label());
                    }
                });
            });
        });
        if start {
            self.on_start_device();
        }
        if stop {
            self.on_stop_device();
        }

        // ----- manage the selected device: RAM, cores, saved state -----
        let selected: Option<DeviceInfo> = self.sel_device.and_then(|i| self.devices.get(i).cloned());
        let sel_name = selected.as_ref().map(|d| d.name.clone());
        if sel_name != self.edit_for {
            self.edit_for = sel_name;
            if let Some(d) = &selected {
                self.edit_ram = devices::leading_number(&d.ram).unwrap_or(2048);
                self.edit_cores = devices::leading_number(&d.cores).unwrap_or(2);
            }
        }
        let max_ram = self.pc_ram_mb.map_or(8192, |r| ((r * 3 / 4) as u32).clamp(2048, 16384));
        let max_cores = (self.pc_cores as u32).clamp(1, 8);
        let (mut save, mut fill, mut clear) = (false, false, false);
        egui::CollapsingHeader::new("Manage selected device (RAM, CPU, saved state)").show(ui, |ui| {
            let Some(d) = &selected else {
                ui.label("Select a device in the table first.");
                return;
            };
            let is_running = self.running.as_ref().map_or(false, |r| r.name == d.name);
            if is_running {
                ui.colored_label(RED, "Stop this device first to change it.");
            }
            ui.add_enabled_ui(!is_running && !self.busy, |ui| {
                ui.horizontal(|ui| {
                    ui.label("RAM:");
                    ui.add(egui::Slider::new(&mut self.edit_ram, 1024..=max_ram).step_by(256.0).suffix(" MB"));
                });
                ui.horizontal(|ui| {
                    ui.label("CPU cores:");
                    ui.add(egui::Slider::new(&mut self.edit_cores, 1..=max_cores));
                });
                if let Some(pc) = self.pc_ram_mb {
                    if self.edit_ram as u64 * 2 > pc {
                        ui.colored_label(RED, "That is more than half of your PC's RAM. Windows and other programs may slow down.");
                    }
                }
                ui.horizontal(|ui| {
                    egui::ComboBox::from_id_salt("edit_preset")
                        .selected_text(self.edit_preset.label())
                        .show_ui(ui, |ui| {
                            for p in Preset::ALL {
                                ui.selectable_value(&mut self.edit_preset, p, p.label());
                            }
                        });
                    fill = ui.button("Fill from preset").clicked();
                    save = ui.button("Save changes").clicked();
                });
                ui.weak("Saving also clears the saved state, so the next start is a full boot.");
                ui.horizontal(|ui| {
                    ui.label(format!("Saved state (quick boot): {}", devices::human_size(d.snap_bytes)));
                    clear = ui.add_enabled(d.snap_bytes > 0, egui::Button::new("Clear saved state")).clicked();
                });
            });
        });
        if fill {
            self.edit_ram = self.edit_preset.ram_mb();
            self.edit_cores = self.edit_preset.effective_cores() as u32;
        }
        if save {
            self.on_save_resources();
        }
        if clear {
            self.on_clear_state();
        }

        ui.add_space(8.0);
        ui.separator();
        ui.strong("New device");
        let rec = self.pc_ram_mb.map(|r| devices::recommend(r, self.pc_cores));
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut self.new_name).hint_text("Device name").desired_width(150.0));

            let img_text = self
                .new_image
                .map(|i| self.images[i].display_name())
                .unwrap_or_else(|| "Pick an image".to_string());
            egui::ComboBox::from_id_salt("new_image").selected_text(img_text).show_ui(ui, |ui| {
                for (i, img) in self.images.iter().enumerate() {
                    if self.installed[i] {
                        ui.selectable_value(&mut self.new_image, Some(i), img.display_name());
                    }
                }
            });

            egui::ComboBox::from_id_salt("new_preset")
                .selected_text(self.new_preset.label())
                .show_ui(ui, |ui| {
                    for p in Preset::ALL {
                        let text = if rec == Some(p) {
                            format!("{} (suggested)", p.label())
                        } else {
                            p.label().to_string()
                        };
                        ui.selectable_value(&mut self.new_preset, p, text);
                    }
                });
        });
        ui.label(self.new_preset.description());
        if let Some(ram) = self.pc_ram_mb {
            ui.weak(format!(
                "Your PC: {:.0} GB RAM, {} CPU threads. Suggested preset: {}.",
                ram as f64 / 1024.0,
                self.pc_cores,
                devices::recommend(ram, self.pc_cores).label()
            ));
        }
        ui.add_space(8.0);

        let (mut create, mut delete, mut refresh) = (false, false, false);
        ui.horizontal(|ui| {
            create = ui
                .add_enabled(
                    !self.busy && self.tools_ok && any_installed,
                    egui::Button::new("Create device"),
                )
                .clicked();
            delete = ui
                .add_enabled(!self.busy && self.tools_ok, egui::Button::new("Delete selected"))
                .clicked();
            refresh = ui.add_enabled(!self.busy, egui::Button::new("Refresh")).clicked();
        });
        if create {
            self.on_create_device();
        }
        if delete {
            self.on_delete_device();
        }
        if refresh {
            self.refresh(false);
        }
    }

    fn ui_tools(&mut self, ui: &mut egui::Ui) {
        ui.heading("ADB Tools");
        if !self.adb_ok {
            ui.colored_label(RED, "Set up the Android SDK first (Setup tab).");
        } else if self.adb_connected {
            ui.colored_label(GREEN, "Emulator connected");
        } else {
            ui.colored_label(
                RED,
                "No running emulator found. Start a device (Devices tab), wait until it finishes booting, then press Refresh.",
            );
        }
        ui.add_space(6.0);

        let can_use = !self.busy && self.adb_ok;
        let mut c = ToolClicks::default();

        let filter = self.app_filter.to_lowercase();
        let shown: Vec<&String> = self.apps.iter().filter(|a| a.to_lowercase().contains(&filter)).collect();

        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            // ----- install -----
            ui.strong("Install an app");
            ui.label("Drag an .apk file onto this window, or paste its path:");
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.apk_path)
                        .hint_text(r"C:\path\to\app.apk")
                        .desired_width(360.0),
                );
                c.install = ui.add_enabled(can_use, egui::Button::new("Install")).clicked();
            });

            // ----- screenshot -----
            ui.add_space(8.0);
            ui.separator();
            ui.strong("Screenshot");
            ui.horizontal(|ui| {
                c.shot = ui.add_enabled(can_use, egui::Button::new("Take screenshot")).clicked();
                c.open_shots = ui.button("Open screenshots folder").clicked();
            });

            // ----- installed apps -----
            ui.add_space(8.0);
            ui.separator();
            ui.strong("Installed apps");
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut self.app_filter).hint_text("Filter").desired_width(200.0));
                c.system_changed = ui.checkbox(&mut self.show_system, "Show system apps").changed();
                c.refresh = ui.add_enabled(!self.busy, egui::Button::new("Refresh")).clicked();
            });
            egui::ScrollArea::vertical()
                .id_salt("apps_list")
                .max_height(180.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    if shown.is_empty() {
                        ui.label(if self.adb_connected { "No apps to show." } else { "-" });
                    }
                    for pkg in &shown {
                        let selected = self.sel_app.as_deref() == Some(pkg.as_str());
                        if ui.selectable_label(selected, pkg.as_str()).clicked() {
                            self.sel_app = Some((*pkg).clone());
                        }
                    }
                });
            c.uninstall = ui.add_enabled(can_use, egui::Button::new("Uninstall selected")).clicked();

            // ----- shell -----
            ui.add_space(8.0);
            ui.separator();
            ui.strong("Run a command in the emulator (adb shell)");
            ui.horizontal(|ui| {
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut self.shell_cmd)
                        .hint_text("e.g. getprop ro.build.version.release")
                        .desired_width(420.0),
                );
                let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                let clicked = ui.add_enabled(can_use, egui::Button::new("Run")).clicked();
                c.run_shell = clicked || (enter && can_use);
            });
            ui.horizontal_wrapped(|ui| {
                ui.label("Examples:");
                for (label, cmd) in SHELL_EXAMPLES {
                    if ui.small_button(label).clicked() {
                        self.shell_cmd = cmd.to_string();
                    }
                }
            });
            ui.weak("The output appears in the log below. Commands run on the emulator only, never on your PC.");

            // ----- files -----
            ui.add_space(8.0);
            ui.separator();
            ui.strong("Copy files");
            ui.label("PC to emulator (you can also drag any file onto this window):");
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.push_src)
                        .hint_text(r"C:\path\to\file")
                        .desired_width(260.0),
                );
                ui.label("to");
                ui.add(egui::TextEdit::singleline(&mut self.push_dest).desired_width(160.0));
                c.push = ui.add_enabled(can_use, egui::Button::new("Copy to emulator")).clicked();
            });
            ui.label("Emulator to PC:");
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.pull_src)
                        .hint_text("/sdcard/Download/file.txt")
                        .desired_width(260.0),
                );
                c.pull = ui.add_enabled(can_use, egui::Button::new("Copy to PC")).clicked();
                c.open_pulled = ui.button("Open folder").clicked();
            });
            ui.weak("Files copied to the PC go to Downloads\\AndroidEmulatorManager.");
            ui.add_space(8.0);
        });

        if c.install {
            let path = adb::clean_path(&self.apk_path);
            if path.is_empty() {
                self.info = Some("Drop an .apk file here, or paste its path first.".to_string());
            } else {
                self.on_install_apk(PathBuf::from(path));
            }
        }
        if c.shot {
            self.on_screenshot();
        }
        if c.open_shots {
            adb::open_folder(&paths::screenshots_dir());
        }
        if c.uninstall {
            self.on_uninstall_app();
        }
        if c.refresh || c.system_changed {
            self.adb_refresh();
        }
        if c.run_shell {
            self.on_run_shell();
        }
        if c.push {
            self.on_push();
        }
        if c.pull {
            self.on_pull();
        }
        if c.open_pulled {
            adb::open_folder(&paths::pulled_dir());
        }
    }

    fn ui_keymap(&mut self, ui: &mut egui::Ui) {
        ui.heading("Keymap");
        let enabled = self.km_runtime.is_some();
        let can_use = !self.busy && self.adb_ok;
        let mut k = KmClicks::default();

        // --- recording a binding: wait for the next key press ---
        if let Some((fx, fy)) = self.km_pending {
            let events = ui.input(|i| i.events.clone());
            for ev in events {
                if let egui::Event::Key { key, pressed: true, repeat: false, .. } = ev {
                    if key == egui::Key::Escape {
                        self.km_pending = None;
                    } else if key == egui::Key::F10 {
                        self.info = Some("F10 is reserved: it pauses and resumes mapping.".to_string());
                    } else if let Some(label) = keymap::label_for_egui(key) {
                        if let Some(p) = self.km.as_mut() {
                            p.set(label, fx, fy);
                        }
                        self.km_pending = None;
                        k.save = true;
                    } else {
                        self.info = Some(
                            "That key can't be used. Try a letter, number, arrow key, Space, or Enter.".to_string(),
                        );
                    }
                }
            }
        }

        // --- status ---
        if !self.adb_ok {
            ui.colored_label(RED, "Set up the Android SDK first (Setup tab).");
        } else if !self.adb_connected {
            ui.colored_label(RED, "No running emulator found. Start a device, wait until it finishes booting, then press Refresh.");
        } else if enabled && self.km_active {
            ui.colored_label(
                GREEN,
                format!("Mapping is ON. Click into the emulator window and play. Press {} there to pause or resume.", keymap::TOGGLE_LABEL),
            );
        } else if enabled {
            ui.colored_label(
                ORANGE,
                format!("Mapping is PAUSED. Press {} in the emulator window to resume.", keymap::TOGGLE_LABEL),
            );
        } else {
            ui.weak("Mapping is off.");
        }
        ui.weak("Taps arrive about 0.1-0.3 s late and one spot at a time (no joystick). While mapping is on, mapped keys do not reach the emulator.");
        ui.add_space(6.0);

        // --- profile row ---
        let cur_name = self.km.as_ref().map(|p| p.name.clone());
        ui.horizontal(|ui| {
            ui.label("Profile:");
            ui.add_enabled_ui(!enabled, |ui| {
                egui::ComboBox::from_id_salt("km_profile")
                    .selected_text(cur_name.clone().unwrap_or_else(|| "(none)".to_string()))
                    .show_ui(ui, |ui| {
                        for n in &self.km_profiles {
                            if ui.selectable_label(cur_name.as_deref() == Some(n.as_str()), n).clicked() {
                                k.select = Some(n.clone());
                            }
                        }
                    });
            });
            ui.add(
                egui::TextEdit::singleline(&mut self.km_new_name)
                    .hint_text("New profile (e.g. game name)")
                    .desired_width(190.0),
            );
            k.create = ui.add_enabled(!enabled, egui::Button::new("Create")).clicked();
            k.delete = ui.add_enabled(cur_name.is_some() && !enabled, egui::Button::new("Delete")).clicked();
        });

        // --- action row ---
        let can_toggle = self.adb_ok
            && !self.km_starting
            && (enabled || self.km.as_ref().map_or(false, |p| !p.binds.is_empty()));
        ui.horizontal(|ui| {
            k.capture = ui.add_enabled(can_use && !enabled, egui::Button::new("Capture screen")).clicked();
            let label = if enabled { "Turn mapping OFF" } else { "Turn mapping ON" };
            k.toggle = ui.add_enabled(can_toggle, egui::Button::new(label)).clicked();
            if enabled {
                let pause_label = if self.km_active { "Pause" } else { "Resume" };
                k.pause = ui.button(pause_label).clicked();
            }
            k.refresh = ui.add_enabled(!self.busy, egui::Button::new("Refresh")).clicked();
        });
        ui.add_space(6.0);

        // --- screen picture + bindings ---
        if self.km.is_none() {
            ui.label("Create a profile first (one per game works best), then press \"Capture screen\" while the game is running.");
        } else {
            let editing = !enabled;
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| match (&self.km_tex, self.km_dev_size) {
                    (Some(tex), Some((dw, dh))) => {
                        let max = egui::vec2(520.0, 460.0);
                        let scale = (max.x / dw as f32).min(max.y / dh as f32);
                        let size = egui::vec2(dw as f32 * scale, dh as f32 * scale);
                        let resp = ui.add(egui::Image::new((tex.id(), size)).sense(egui::Sense::click()));

                        let painter = ui.painter_at(resp.rect);
                        if let Some(p) = &self.km {
                            for b in &p.binds {
                                let pos = resp.rect.min + egui::vec2(b.x * size.x, b.y * size.y);
                                painter.circle_filled(pos, 12.0, egui::Color32::from_rgba_unmultiplied(61, 220, 132, 220));
                                let short: String = b.key.chars().take(3).collect();
                                painter.text(pos, egui::Align2::CENTER_CENTER, short, egui::FontId::proportional(11.0), egui::Color32::BLACK);
                            }
                        }
                        if let Some((px, py)) = self.km_pending {
                            let pos = resp.rect.min + egui::vec2(px * size.x, py * size.y);
                            painter.circle_stroke(pos, 14.0, egui::Stroke::new(2.5_f32, ORANGE));
                        }
                        if editing && resp.clicked() {
                            if let Some(pos) = resp.interact_pointer_pos() {
                                let fx = ((pos.x - resp.rect.min.x) / size.x).clamp(0.0, 1.0);
                                let fy = ((pos.y - resp.rect.min.y) / size.y).clamp(0.0, 1.0);
                                k.clicked_at = Some((fx, fy));
                            }
                        }
                    }
                    _ => {
                        ui.label("Press \"Capture screen\" while your game is running,\nthen click where each on-screen button is.");
                    }
                });

                ui.vertical(|ui| {
                    ui.set_min_width(260.0);
                    ui.strong("Key bindings");
                    if self.km_pending.is_some() {
                        ui.colored_label(ORANGE, "Now press the key for the spot you clicked (Esc cancels).");
                    } else if editing {
                        ui.weak("Click a spot on the screen, then press a key.");
                    } else {
                        ui.weak("Turn mapping off to edit.");
                    }
                    egui::ScrollArea::vertical().id_salt("km_binds").max_height(400.0).show(ui, |ui| {
                        if let Some(p) = &self.km {
                            for (i, b) in p.binds.iter().enumerate() {
                                ui.horizontal(|ui| {
                                    ui.monospace(format!("{:<9}", b.key));
                                    ui.label(format!("{:.0}%, {:.0}%", b.x * 100.0, b.y * 100.0));
                                    let can_test = can_use && !enabled && self.km_dev_size.is_some();
                                    if ui.add_enabled(can_test, egui::Button::new("Test")).clicked() {
                                        k.test = Some(i);
                                    }
                                    if ui.add_enabled(editing, egui::Button::new("X")).clicked() {
                                        k.remove = Some(i);
                                    }
                                });
                            }
                        }
                    });
                });
            });
        }

        // --- act on clicks ---
        if let Some(n) = k.select.take() {
            self.km_select(&n);
        }
        if k.create {
            self.km_create();
        }
        if k.delete {
            if let Some(n) = cur_name {
                self.confirm = Some((format!("Delete the keymap profile \"{n}\"?"), Action::DeleteProfile(n)));
            }
        }
        if k.capture {
            self.km_capture();
        }
        if k.refresh {
            self.adb_refresh();
        }
        if k.toggle {
            self.km_toggle();
        }
        if k.pause {
            self.km_active = !self.km_active;
            hook::set_active(self.km_active);
        }
        if let Some(pos) = k.clicked_at {
            self.km_pending = Some(pos);
        }
        if let Some(i) = k.remove {
            if let Some(p) = self.km.as_mut() {
                if i < p.binds.len() {
                    p.binds.remove(i);
                }
            }
            k.save = true;
        }
        if let Some(i) = k.test {
            self.km_test(i);
        }
        if k.save {
            self.km_save();
        }
    }

    // ---------- dialogs ----------

    fn dialogs(&mut self, ctx: &egui::Context) {
        // confirm
        let mut decision: Option<bool> = None;
        if let Some((text, _)) = &self.confirm {
            egui::Window::new("Please confirm")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label(text);
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("OK").clicked() {
                            decision = Some(true);
                        }
                        if ui.button("Cancel").clicked() {
                            decision = Some(false);
                        }
                    });
                });
        }
        match decision {
            Some(true) => {
                if let Some((_, action)) = self.confirm.take() {
                    self.run_action(action);
                }
            }
            Some(false) => self.confirm = None,
            None => {}
        }

        // info
        let mut close = false;
        if let Some(text) = &self.info {
            egui::Window::new("Info")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label(text);
                    ui.add_space(8.0);
                    if ui.button("OK").clicked() {
                        close = true;
                    }
                });
        }
        if close {
            self.info = None;
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain();

        // Don't leave an emulator running (with a closed log pipe) when the manager closes.
        if ctx.input(|i| i.viewport().close_requested()) && !self.quitting && self.running.is_some() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.confirm = Some((
                "An emulator is running. Stop it and quit?".to_string(),
                Action::StopAndQuit,
            ));
        }

        let before = self.tab;
        egui::TopBottomPanel::top("tabs").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.tab, Tab::Setup, "Setup");
                ui.selectable_value(&mut self.tab, Tab::Images, "System Images");
                ui.selectable_value(&mut self.tab, Tab::Devices, "Devices");
                ui.selectable_value(&mut self.tab, Tab::Tools, "ADB Tools");
                ui.selectable_value(&mut self.tab, Tab::Keymap, "Keymap");
            });
        });
        if self.tab != before {
            if self.tab == Tab::Tools || self.tab == Tab::Keymap {
                self.adb_refresh();
            } else {
                self.refresh(false);
            }
        }

        egui::TopBottomPanel::bottom("log").resizable(true).default_height(170.0).show(ctx, |ui| {
            if self.busy {
                ui.horizontal(|ui| match self.progress {
                    Some(p) => {
                        ui.add(egui::ProgressBar::new(p).show_percentage());
                    }
                    None => {
                        ui.spinner();
                        ui.label("Working...");
                    }
                });
            }
            egui::ScrollArea::vertical().stick_to_bottom(true).auto_shrink([false, false]).show(ui, |ui| {
                for line in &self.log {
                    ui.label(egui::RichText::new(line).monospace());
                }
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| match self.tab {
            Tab::Setup => self.ui_setup(ui),
            Tab::Images => self.ui_images(ui),
            Tab::Devices => self.ui_devices(ui),
            Tab::Tools => self.ui_tools(ui),
            Tab::Keymap => self.ui_keymap(ui),
        });

        // Drag & drop an .apk anywhere on the window to install it.
        let dropped: Vec<egui::DroppedFile> = ctx.input(|i| i.raw.dropped_files.clone());
        if let Some(path) = dropped.into_iter().find_map(|f| f.path) {
            let is_apk = path.extension().map_or(false, |e| e.eq_ignore_ascii_case("apk"));
            if is_apk {
                self.on_install_apk(path);
            } else {
                self.push_src = path.to_string_lossy().to_string();
                self.tab = Tab::Tools;
                self.adb_refresh();
                self.push_log(format!(
                    "Ready to copy: {} (press \"Copy to emulator\" on the ADB Tools tab)",
                    path.display()
                ));
            }
        }
        if ctx.input(|i| !i.raw.hovered_files.is_empty()) {
            let rect = ctx.screen_rect();
            let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("drop_overlay")));
            painter.rect_filled(rect, 0.0, egui::Color32::from_black_alpha(170));
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "Drop an .apk to install it, or any file to copy it to the emulator",
                egui::FontId::proportional(24.0),
                egui::Color32::WHITE,
            );
        }

        self.dialogs(ctx);
    }
}

// ---------- small helpers ----------

fn status_row(ui: &mut egui::Ui, name: &str, ok: bool, text: &str) {
    ui.strong(name);
    let (prefix, color) = if ok { ("OK - ", GREEN) } else { ("Missing - ", RED) };
    ui.colored_label(color, format!("{prefix}{text}"));
    ui.end_row();
}

fn yes_no(ok: bool) -> &'static str {
    if ok { "Installed" } else { "Not installed" }
}

/// Turn "system-images/android-34/google_apis/x86_64/" into a readable name when we know it.
fn friendly(images: &[SystemImage], sysdir: &str) -> String {
    let norm = devices::normalize_sysdir(sysdir);
    for img in images {
        if devices::normalize_sysdir(&img.package_id.replace(';', "/")) == norm {
            return img.display_name();
        }
    }
    sysdir.to_string()
}
