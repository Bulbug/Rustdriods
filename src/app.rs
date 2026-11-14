//! The window. Three tabs (Setup, System Images, Devices) and one shared log panel.
//! Heavy work runs on background threads and reports back through a channel.
use crate::adb;
use crate::devices::{self, DeviceInfo, Preset};
use crate::images::{self, SystemImage};
use crate::launcher::{self, Gpu, LaunchOptions};
use crate::{java, paths, sdk, virt, Res};
use eframe::egui;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::Duration;

const GREEN: egui::Color32 = egui::Color32::from_rgb(26, 127, 55);
const RED: egui::Color32 = egui::Color32::from_rgb(198, 40, 40);

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
                Msg::EmuExited { code } => {
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
            });
        });
        if self.tab != before {
            if self.tab == Tab::Tools {
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
