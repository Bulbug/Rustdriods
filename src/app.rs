//! The window. Three tabs (Setup, System Images, Devices) and one shared log panel.
//! Heavy work runs on background threads and reports back through a channel.
use crate::adb;
use crate::keymap::{self, Profile};
use crate::theme::{self, Mode};
use crate::{hook, inject};

mod views;
use crate::devices::{self, DeviceInfo, Preset};
use crate::images::{self, SystemImage};
use crate::launcher::{self, Gpu, LaunchOptions};
use crate::optimize::{self, OptimizeOptions};
use crate::{java, paths, sdk, virt, Res};
use eframe::egui;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::Duration;


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
    gpus: Vec<String>,
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
    optimize: bool,
    restore: bool,
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

    gpus: Vec<String>,
    gpu_auto_set: bool,
    opt: OptimizeOptions,
    pc_ram_mb: Option<u64>,
    pc_cores: usize,
    preset_auto_set: bool,
    edit_for: Option<String>,
    edit_ram: u32,
    edit_cores: u32,
    edit_preset: Preset,

    mode: Mode,
    show_new: bool,

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
        let mode = crate::settings::load_mode();
        theme::install(&cc.egui_ctx, mode);
        Self::with_context(&cc.egui_ctx, mode, true)
    }

    /// `refresh` = read the PC state in the background right away (switched off in tests).
    pub fn with_context(ctx: &egui::Context, mode: Mode, refresh: bool) -> Self {
        let (tx, rx) = mpsc::channel();
        let images = images::catalog();
        let app = App {
            rep: Reporter { tx, ctx: ctx.clone() },
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
            gpus: Vec::new(),
            gpu_auto_set: false,
            opt: OptimizeOptions::default(),
            pc_ram_mb: None,
            pc_cores: thread::available_parallelism().map(|n| n.get()).unwrap_or(2),
            preset_auto_set: false,
            edit_for: None,
            edit_ram: 2048,
            edit_cores: 2,
            edit_preset: Preset::Lite,
            mode,
            show_new: false,
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
        if refresh {
            app.refresh(true);
        }
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
                    gpus: virt::gpu_names(),
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
            self.gpus = env.gpus;
            if !self.gpu_auto_set && !self.gpus.is_empty() {
                self.opts.gpu = launcher::advise_gpu(&self.gpus).gpu;
                self.gpu_auto_set = true;
            }
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

    /// Applies (or undoes) the speed tweaks on the running emulator.
    fn on_optimize(&mut self, restore: bool) {
        if self.busy || !self.adb_ok {
            return;
        }
        let opts = self.opt.clone();
        self.spawn_job(false, move |rep| {
            adb::require_connected()?;
            let log = rep.logger();
            // Only touch Google apps that really exist (or, when undoing, that are really turned off).
            let present: Vec<String> = if opts.disable_google_apps {
                let list = if restore { adb::list_disabled()? } else { adb::list_packages(true)? };
                list.into_iter()
                    .filter(|p| optimize::OPTIONAL_GOOGLE_APPS.contains(&p.as_str()))
                    .collect()
            } else {
                Vec::new()
            };
            optimize::run(&optimize::plan(&opts, restore, &present), &log)
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
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.draw(ctx);
    }
}

// ---------- small helpers ----------

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicU32};

    fn frame(ctx: &egui::Context, app: &mut App) {
        let mut raw = egui::RawInput::default();
        raw.screen_rect = Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 680.0)));
        let _ = ctx.run(raw, |ctx| app.draw(ctx));
    }

    fn device(name: &str, snap: u64) -> DeviceInfo {
        DeviceInfo {
            name: name.into(),
            sysdir: "system-images/android-34/google_apis/x86_64/".into(),
            ram: "1536".into(),
            cores: "2".into(),
            screen: "540x960".into(),
            snap_bytes: snap,
        }
    }

    #[test]
    fn every_page_draws_in_light_and_dark() {
        for mode in [Mode::Light, Mode::Dark] {
            let ctx = egui::Context::default();
            theme::install(&ctx, mode);
            let mut app = App::with_context(&ctx, mode, false);
            app.tools_ok = true;
            app.adb_ok = true;
            app.emu_ok = true;
            app.java_ok = true;
            app.adb_connected = true;
            app.virt = (true, "Ready".into());
            app.gpus = vec!["NVIDIA GeForce GTX 1650".into()];
            app.pc_ram_mb = Some(16384);
            app.installed[2] = true;
            app.new_image = Some(2);
            app.devices = vec![device("Pixel_Lite", 1_500_000_000), device("Pixel_Two", 0)];
            app.running = Some(Running {
                name: "Pixel_Lite".into(),
                pid: Arc::new(AtomicU32::new(0)),
                alive: Arc::new(AtomicBool::new(true)),
                stopping: false,
            });
            app.apps = vec!["com.example.app".into(), "com.other.app".into()];
            app.sel_device = Some(0);
            app.sel_image = Some(2);
            app.show_new = true;
            app.km = Some(Profile {
                name: "Game".into(),
                landscape: false,
                binds: vec![keymap::Binding { key: "W".into(), x: 0.5, y: 0.5 }],
            });
            app.km_profiles = vec!["Game".into()];
            app.log = vec!["first line".into(), "second line".into()];
            app.busy = true;
            app.progress = Some(0.4);
            app.confirm = Some(("Are you sure?".into(), Action::Setup));
            app.info = Some("Hello".into());
            let img = egui::ColorImage::new([4, 4], egui::Color32::from_rgb(90, 90, 240));
            app.km_tex = Some(ctx.load_texture("test_screen", img, egui::TextureOptions::LINEAR));
            app.km_pending = Some((0.4, 0.6));
            for (size, extra) in [((1080, 1920), 0), ((1920, 1080), 1)] {
                app.km_dev_size = Some(size);
                app.tab = Tab::Keymap;
                app.km_active = extra == 0;
                frame(&ctx, &mut app);
                frame(&ctx, &mut app);
            }
            app.km_pending = None;
            for tab in [Tab::Setup, Tab::Images, Tab::Devices, Tab::Tools, Tab::Keymap] {
                app.tab = tab;
                for _ in 0..3 {
                    frame(&ctx, &mut app);
                }
            }
            // empty / first-run state too
            let mut fresh = App::with_context(&ctx, mode, false);
            for tab in [Tab::Setup, Tab::Images, Tab::Devices, Tab::Tools, Tab::Keymap] {
                fresh.tab = tab;
                frame(&ctx, &mut fresh);
            }
        }
    }
}
