//! The window layout (sidebar, log bar) and the Setup, System Images and Devices pages.
use super::*;
use crate::icons::{self, Icon};
use crate::theme::{self, Mode, Pal};
use crate::widgets::{self, Kind, Seg};
use eframe::egui::{Align, Frame, Layout, Margin, RichText, Rounding, Sense, Stroke};

// ---------- sidebar pieces ----------

fn nav_item(ui: &mut egui::Ui, p: &Pal, icon: Icon, label: &str, active: bool) -> bool {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 44.0), Sense::click());
    let bg = if active {
        p.tint
    } else if resp.hovered() {
        p.bg
    } else {
        p.sd
    };
    ui.painter().rect_filled(rect, Rounding::same(12.0), bg);
    let color = if active { p.tintx } else { p.mu };
    icons::paint(ui.painter(), icon, egui::pos2(rect.left() + 26.0, rect.center().y), 18.0, color, bg);
    ui.painter().text(
        egui::pos2(rect.left() + 48.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::new(13.5, theme::bold_family()),
        color,
    );
    resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

/// Light / Dark / Match Windows. Returns true when the choice changed.
fn theme_toggle(ui: &mut egui::Ui, p: &Pal, mode: &mut Mode) -> bool {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 40.0), Sense::hover());
    ui.painter().rect(rect, Rounding::same(20.0), p.bg, Stroke::new(1.0_f32, p.ln));
    let seg_w = (rect.width() - 8.0) / 3.0;
    let mut changed = false;
    for (i, (m, icon, tip)) in
        [(Mode::Light, Icon::Sun, "Light"), (Mode::Dark, Icon::Moon, "Dark"), (Mode::Auto, Icon::Monitor, "Match Windows")]
            .into_iter()
            .enumerate()
    {
        let r = egui::Rect::from_min_size(
            egui::pos2(rect.left() + 4.0 + i as f32 * seg_w, rect.top() + 4.0),
            egui::vec2(seg_w, 32.0),
        );
        let resp = ui.interact(r, ui.id().with(("theme", i)), Sense::click()).on_hover_text(tip);
        let active = *mode == m;
        if active {
            ui.painter().rect_filled(r, Rounding::same(16.0), p.ac);
        }
        let (fg, bg) = if active { (p.on, p.ac) } else { (p.mu, p.bg) };
        icons::paint(ui.painter(), icon, r.center(), 16.0, fg, bg);
        if resp.clicked() && *mode != m {
            *mode = m;
            changed = true;
        }
    }
    changed
}

impl App {
    fn sidebar(&mut self, ui: &mut egui::Ui, p: &Pal) -> bool {
        ui.spacing_mut().item_spacing.y = 4.0;
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(egui::vec2(32.0, 32.0), Sense::hover());
            ui.painter().rect_filled(r, Rounding::same(10.0), p.ac);
            icons::paint(ui.painter(), Icon::Devices, r.center(), 18.0, p.on, p.ac);
            ui.label(widgets::bold("Emulator Manager", 15.0));
        });
        ui.add_space(18.0);
        for (tab, icon, label) in [
            (Tab::Setup, Icon::Setup, "Setup"),
            (Tab::Images, Icon::Images, "System Images"),
            (Tab::Devices, Icon::Devices, "Devices"),
            (Tab::Tools, Icon::Adb, "ADB Tools"),
            (Tab::Keymap, Icon::Keymap, "Keymap"),
        ] {
            if nav_item(ui, p, icon, label, self.tab == tab) {
                self.tab = tab;
            }
        }
        let mut changed = false;
        ui.with_layout(Layout::bottom_up(Align::Center), |ui| {
            ui.label(RichText::new(format!("v{}", env!("CARGO_PKG_VERSION"))).color(p.mu).small());
            ui.add_space(6.0);
            changed = theme_toggle(ui, p, &mut self.mode);
        });
        changed
    }

    /// Draws one whole frame of the window.
    pub(super) fn draw(&mut self, ctx: &egui::Context) {
        self.drain();

        // Don't leave an emulator running (with a closed log pipe) when the manager closes.
        if ctx.input(|i| i.viewport().close_requested()) && !self.quitting && self.running.is_some() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.confirm = Some(("An emulator is running. Stop it and quit?".to_string(), Action::StopAndQuit));
        }

        let p = if ctx.style().visuals.dark_mode { theme::DARK } else { theme::LIGHT };
        let before = self.tab;
        let mut mode_changed = false;

        egui::SidePanel::left("nav")
            .exact_width(216.0)
            .resizable(false)
            .frame(Frame::none().fill(p.sd).stroke(Stroke::new(1.0_f32, p.ln)).inner_margin(Margin::symmetric(14.0, 22.0)))
            .show(ctx, |ui| {
                mode_changed = self.sidebar(ui, &p);
            });
        if mode_changed {
            theme::set_mode(ctx, self.mode);
            crate::settings::save_mode(self.mode);
        }
        if self.tab != before {
            if self.tab == Tab::Tools || self.tab == Tab::Keymap {
                self.adb_refresh();
            } else {
                self.refresh(false);
            }
        }

        let mut clear_log = false;
        egui::TopBottomPanel::bottom("log")
            .resizable(true)
            .default_height(118.0)
            .min_height(44.0)
            .frame(Frame::none().fill(p.sd).stroke(Stroke::new(1.0_f32, p.ln)).inner_margin(Margin::symmetric(36.0, 10.0)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(widgets::bold("Log", 13.0));
                    if self.busy {
                        match self.progress {
                            Some(f) => {
                                ui.add(egui::ProgressBar::new(f).desired_width(200.0).fill(p.ac).show_percentage());
                            }
                            None => {
                                ui.spinner();
                                ui.label(RichText::new("Working...").color(p.mu));
                            }
                        }
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        clear_log = widgets::link(ui, &p, "Clear").clicked();
                    });
                });
                egui::ScrollArea::vertical().stick_to_bottom(true).auto_shrink([false, false]).show(ui, |ui| {
                    for line in &self.log {
                        ui.label(RichText::new(line).monospace().color(p.mu));
                    }
                });
            });
        if clear_log {
            self.log.clear();
        }

        egui::CentralPanel::default()
            .frame(Frame::none().fill(p.bg).inner_margin(Margin::symmetric(36.0, 28.0)))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| match self.tab {
                    Tab::Setup => self.ui_setup(ui),
                    Tab::Images => self.ui_images(ui),
                    Tab::Devices => self.ui_devices(ui),
                    Tab::Tools => self.ui_tools(ui),
                    Tab::Keymap => self.ui_keymap(ui),
                });
            });

        // Drag & drop: an .apk installs, any other file is queued for "Copy to emulator".
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
                    "Ready to copy: {} (press \"Copy to emulator\" on the ADB Tools page)",
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
                egui::FontId::new(24.0, theme::bold_family()),
                egui::Color32::WHITE,
            );
        }

        self.dialogs(ctx, &p);
    }

    fn dialogs(&mut self, ctx: &egui::Context, p: &Pal) {
        let mut decision: Option<bool> = None;
        if let Some((text, _)) = &self.confirm {
            egui::Window::new("Please confirm")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .max_width(460.0)
                .show(ctx, |ui| {
                    ui.label(text);
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if widgets::primary(ui, p, "OK", true, 84.0).clicked() {
                            decision = Some(true);
                        }
                        if widgets::ghost(ui, p, "Cancel", true, 84.0).clicked() {
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

        let mut close = false;
        if let Some(text) = &self.info {
            egui::Window::new("Info")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .max_width(460.0)
                .show(ctx, |ui| {
                    ui.label(text);
                    ui.add_space(8.0);
                    close = widgets::primary(ui, p, "OK", true, 84.0).clicked();
                });
        }
        if close {
            self.info = None;
        }
    }

    // ---------- Setup ----------

    pub(super) fn ui_setup(&mut self, ui: &mut egui::Ui) {
        let p = theme::pal(ui);
        let advice = launcher::advise_gpu(&self.gpus);
        let gpu_known = !self.gpus.is_empty();
        let tools_ready = self.adb_ok && self.emu_ok;
        let items: Vec<(&str, String, bool)> = vec![
            ("Virtualization", self.virt.1.clone(), self.virt.0),
            (
                "Graphics",
                if gpu_known { format!("{} - {}", self.gpus.join(", "), advice.note) } else { "Checking...".to_string() },
                gpu_known && advice.ok,
            ),
            (
                "Java 17+ (for the SDK tools)",
                if self.java_ok { "Found".to_string() } else { "Not found - a small runtime (~50 MB) will be downloaded".to_string() },
                self.java_ok,
            ),
            ("SDK command-line tools", yes_no(self.tools_ok).to_string(), self.tools_ok),
            ("Android Emulator and adb", yes_no(tools_ready).to_string(), tools_ready),
        ];
        let ready = items.iter().filter(|i| i.2).count();
        let all_ready = ready == items.len();
        let mut marked = false;
        let busy = self.busy;
        let segs: Vec<Seg> = items
            .iter()
            .map(|i| {
                if i.2 {
                    Seg::Done
                } else if busy && !marked {
                    marked = true;
                    Seg::Active
                } else {
                    Seg::Todo
                }
            })
            .collect();

        widgets::page_header(ui, &p, "Setup", "Get everything ready to run Android", |_| {});

        widgets::panel(ui, &p, |ui| {
            ui.horizontal(|ui| {
                ui.label(widgets::bold(format!("{ready} of {} ready", items.len()), 22.0));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if self.busy {
                        let t = match self.progress {
                            Some(f) => format!("Downloading... {:.0}%", f * 100.0),
                            None => "Working...".to_string(),
                        };
                        ui.label(RichText::new(t).color(p.mu));
                    }
                });
            });
            widgets::segments(ui, &p, &segs);
        });

        widgets::panel(ui, &p, |ui| {
            for (i, (name, detail, ok)) in items.iter().enumerate() {
                if i > 0 {
                    ui.separator();
                }
                ui.horizontal(|ui| {
                    widgets::status_icon(ui, &p, *ok);
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        ui.label(widgets::bold(*name, 14.0));
                        ui.label(RichText::new(detail).color(p.mu));
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if *ok {
                            widgets::chip(ui, &p, Kind::Ok, "Ready");
                        } else {
                            widgets::chip(ui, &p, Kind::Warn, "Needs setup");
                        }
                    });
                });
            }
        });

        let (mut setup, mut check) = (false, false);
        ui.horizontal(|ui| {
            setup = widgets::primary(ui, &p, "Set up Android SDK", !self.busy && !all_ready, 0.0).clicked();
            check = widgets::ghost(ui, &p, "Check again", !self.busy, 0.0).clicked();
            ui.label(RichText::new("By continuing you accept Google's Android SDK License.").color(p.mu).small());
        });
        if setup {
            self.confirm = Some((SETUP_TEXT.to_string(), Action::Setup));
        }
        if check {
            self.refresh(true);
        }

        ui.add_space(10.0);
        ui.label(RichText::new(format!("Data folder: {}", paths::home().display())).color(p.mu).small());
        if widgets::link(ui, &p, "Open data folder").clicked() {
            adb::open_folder(&paths::home());
        }
    }

    // ---------- System Images ----------

    pub(super) fn ui_images(&mut self, ui: &mut egui::Ui) {
        let p = theme::pal(ui);
        let can = !self.busy && self.tools_ok;
        let (mut install, mut delete, mut refresh) = (false, false, false);
        widgets::page_header(ui, &p, "System Images", "Choose the Android versions you want to use", |ui| {
            install = widgets::primary(ui, &p, "Install selected", can, 0.0).clicked();
            delete = widgets::danger(ui, &p, "Delete selected", can, 0.0).clicked();
            refresh = widgets::ghost(ui, &p, "Refresh", !self.busy, 0.0).clicked();
        });
        if !self.tools_ok {
            widgets::chip(ui, &p, Kind::Warn, "Set up the Android SDK first (Setup page)");
        }

        let mut select = None;
        for i in 0..self.images.len() {
            let label = self.images[i].android_label();
            let variant = self.images[i].variant;
            let installed = self.installed[i];
            let resp = widgets::row(ui, &p, ("img", i), self.sel_image == Some(i), 56.0, |ui| {
                ui.label(widgets::bold(label, 14.0));
                ui.add_space(6.0);
                widgets::chip(ui, &p, Kind::Accent, variant);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if installed {
                        widgets::chip(ui, &p, Kind::Ok, "Installed");
                    } else {
                        widgets::chip(ui, &p, Kind::Stopped, "Not installed");
                    }
                });
            });
            if resp.clicked() {
                select = Some(i);
            }
        }
        if select.is_some() {
            self.sel_image = select;
        }

        ui.label(
            RichText::new(
                "Google APIs allows root. Google Play has the Play Store but can't be rooted. AOSP has no Google services and is the lightest.",
            )
            .color(p.mu),
        );
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

    // ---------- Devices ----------

    pub(super) fn ui_devices(&mut self, ui: &mut egui::Ui) {
        let p = theme::pal(ui);
        let any_installed = self.installed.iter().any(|&b| b);
        let rec = self.pc_ram_mb.map(|r| devices::recommend(r, self.pc_cores));

        let mut toggle_new = false;
        widgets::page_header(ui, &p, "Devices", "Your virtual phones", |ui| {
            toggle_new = widgets::primary(ui, &p, "+ New device", true, 0.0).clicked();
        });
        if toggle_new {
            self.show_new = !self.show_new;
        }
        if !self.tools_ok {
            widgets::chip(ui, &p, Kind::Warn, "Set up the Android SDK first (Setup page)");
        } else if !any_installed {
            widgets::chip(ui, &p, Kind::Warn, "Install a system image first (System Images page)");
        }

        // --- the three big numbers ---
        let running_n = usize::from(self.running.is_some());
        let stopped_n = self.devices.len().saturating_sub(running_n);
        let total_snap: u64 = self.devices.iter().map(|d| d.snap_bytes).sum();
        let snap_text = if total_snap < (1 << 20) { "0 MB".to_string() } else { devices::human_size(total_snap) };
        ui.columns(3, |c| {
            widgets::stat(&mut c[0], &p, &running_n.to_string(), "Running");
            widgets::stat(&mut c[1], &p, &stopped_n.to_string(), "Stopped");
            widgets::stat(&mut c[2], &p, &snap_text, "Saved state");
        });

        // --- device rows ---
        let (mut select, mut start_i, mut stop) = (None, None, false);
        if self.devices.is_empty() {
            widgets::panel(ui, &p, |ui| {
                ui.label(RichText::new("No devices yet. Press \"+ New device\" to create one.").color(p.mu));
            });
        }
        for i in 0..self.devices.len() {
            let d = self.devices[i].clone();
            let sub = friendly(&self.images, &d.sysdir);
            let is_running = self.running.as_ref().map_or(false, |r| r.name == d.name);
            let stopping = self.running.as_ref().map_or(false, |r| r.stopping);
            let can_start = self.running.is_none() && self.tools_ok && !self.busy;
            let spec = format!("{} · {} cores · {}", devices::ram_label(&d.ram), d.cores, d.screen);
            let resp = widgets::row(ui, &p, ("dev", i), self.sel_device == Some(i), 78.0, |ui| {
                widgets::phone_thumb(ui, &p, egui::vec2(34.0, 56.0));
                ui.add_space(6.0);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 3.0;
                    ui.label(widgets::bold(d.name.clone(), 15.0));
                    ui.label(RichText::new(sub).color(p.mu));
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if is_running {
                        if widgets::danger(ui, &p, "Stop", !stopping, 84.0).clicked() {
                            stop = true;
                        }
                        widgets::chip(ui, &p, Kind::Ok, "Running");
                    } else {
                        if widgets::primary(ui, &p, "Start", can_start, 84.0).clicked() {
                            start_i = Some(i);
                        }
                        widgets::chip(ui, &p, Kind::Stopped, "Stopped");
                    }
                    ui.label(RichText::new(spec).color(p.mu));
                });
            });
            if resp.clicked() {
                select = Some(i);
            }
        }
        if select.is_some() {
            self.sel_device = select;
        }
        if let Some(i) = start_i {
            self.sel_device = Some(i);
            self.on_start_device();
        }
        if stop {
            self.on_stop_device();
        }

        // --- suggestion banner ---
        let mut use_it = false;
        if let (Some(ram), Some(r)) = (self.pc_ram_mb, rec) {
            widgets::banner(ui, &p, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        widgets::bold(
                            format!(
                                "Suggested for your PC ({:.0} GB, {} threads): {} preset - {}",
                                ram as f64 / 1024.0,
                                self.pc_cores,
                                r.label(),
                                r.description()
                            ),
                            13.0,
                        )
                        .color(p.tintx),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        use_it = widgets::link(ui, &p, "Use it").clicked();
                    });
                });
            });
        }
        if use_it {
            if let Some(r) = rec {
                self.new_preset = r;
            }
            self.show_new = true;
        }

        // --- new device ---
        if self.show_new {
            let can_create = !self.busy && self.tools_ok && any_installed;
            let (mut create, mut cancel) = (false, false);
            widgets::panel(ui, &p, |ui| {
                ui.label(widgets::bold("New device", 16.0));
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.new_name).hint_text("Device name").desired_width(190.0));
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
                });
                ui.horizontal_wrapped(|ui| {
                    for pr in Preset::ALL {
                        let text = if rec == Some(pr) { format!("{} (suggested)", pr.label()) } else { pr.label().to_string() };
                        ui.selectable_value(&mut self.new_preset, pr, text);
                    }
                });
                ui.label(RichText::new(self.new_preset.description()).color(p.mu));
                ui.horizontal(|ui| {
                    create = widgets::primary(ui, &p, "Create device", can_create, 0.0).clicked();
                    cancel = widgets::ghost(ui, &p, "Cancel", true, 0.0).clicked();
                });
            });
            if create {
                let ok = !self.new_name.trim().is_empty() && self.new_image.is_some();
                self.on_create_device();
                if ok {
                    self.show_new = false;
                }
            }
            if cancel {
                self.show_new = false;
            }
        }

        // --- manage the selected device ---
        self.ui_manage_device(ui, &p);

        // --- launch options ---
        widgets::panel(ui, &p, |ui| {
            egui::CollapsingHeader::new(widgets::bold("Launch options (lean by default)", 14.0)).show(ui, |ui| {
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
                if !self.gpus.is_empty() {
                    ui.label(
                        RichText::new(format!("Suggested for your PC: {}", launcher::advise_gpu(&self.gpus).gpu.label()))
                            .color(p.mu),
                    );
                }
            });
        });
    }

    fn ui_manage_device(&mut self, ui: &mut egui::Ui, p: &Pal) {
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
        let (mut save, mut fill, mut clear, mut delete) = (false, false, false, false);

        let title = match &selected {
            Some(d) => format!("Manage {} (RAM, CPU, saved state)", d.name),
            None => "Manage a device (RAM, CPU, saved state)".to_string(),
        };
        widgets::panel(ui, p, |ui| {
            egui::CollapsingHeader::new(widgets::bold(title, 14.0)).show(ui, |ui| {
                let Some(d) = &selected else {
                    ui.label(RichText::new("Select a device above first.").color(p.mu));
                    return;
                };
                let is_running = self.running.as_ref().map_or(false, |r| r.name == d.name);
                if is_running {
                    ui.colored_label(p.dg, "Stop this device first to change it.");
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
                            ui.colored_label(p.wn, "That is more than half of your PC's RAM. Windows and other programs may slow down.");
                        }
                    }
                    ui.horizontal(|ui| {
                        egui::ComboBox::from_id_salt("edit_preset").selected_text(self.edit_preset.label()).show_ui(ui, |ui| {
                            for pr in Preset::ALL {
                                ui.selectable_value(&mut self.edit_preset, pr, pr.label());
                            }
                        });
                        fill = widgets::ghost(ui, p, "Fill from preset", true, 0.0).clicked();
                        save = widgets::primary(ui, p, "Save changes", true, 0.0).clicked();
                    });
                    ui.label(RichText::new("Saving also clears the saved state, so the next start is a full boot.").color(p.mu).small());
                    ui.horizontal(|ui| {
                        ui.label(format!("Saved state (quick boot): {}", devices::human_size(d.snap_bytes)));
                        clear = widgets::ghost(ui, p, "Clear saved state", d.snap_bytes > 0, 0.0).clicked();
                    });
                    ui.horizontal(|ui| {
                        delete = widgets::danger(ui, p, "Delete this device", true, 0.0).clicked();
                    });
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
        if delete {
            self.on_delete_device();
        }
    }
}

// ---------- ADB Tools ----------

impl App {
    pub(super) fn ui_tools(&mut self, ui: &mut egui::Ui) {
        let p = theme::pal(ui);
        let can_use = !self.busy && self.adb_ok;
        let mut c = ToolClicks::default();

        widgets::page_header(ui, &p, "ADB Tools", "Install apps, take screenshots, copy files and more", |ui| {
            c.refresh = widgets::ghost(ui, &p, "Refresh", !self.busy, 0.0).clicked();
            if !self.adb_ok {
                widgets::chip(ui, &p, Kind::Warn, "Set up the SDK first");
            } else if self.adb_connected {
                widgets::chip(ui, &p, Kind::Ok, "Emulator connected");
            } else {
                widgets::chip(ui, &p, Kind::Warn, "No running emulator");
            }
        });
        if self.adb_ok && !self.adb_connected {
            ui.label(
                RichText::new("Start a device (Devices page), wait until it finishes booting, then press Refresh.")
                    .color(p.mu),
            );
        }

        // --- install an app ---
        widgets::panel(ui, &p, |ui| {
            ui.label(widgets::bold("Install an app", 16.0));
            widgets::drop_zone(ui, &p, "Drag an .apk file onto this window", "or paste its path below");
            ui.horizontal(|ui| {
                let w = (ui.available_width() - 120.0).max(120.0);
                ui.add(
                    egui::TextEdit::singleline(&mut self.apk_path)
                        .hint_text(r"C:\path\to\app.apk")
                        .desired_width(w),
                );
                c.install = widgets::primary(ui, &p, "Install", can_use, 96.0).clicked();
            });
        });

        // --- screenshot ---
        widgets::panel(ui, &p, |ui| {
            ui.label(widgets::bold("Screenshot", 16.0));
            ui.label(RichText::new("Saved in Pictures\\AndroidEmulatorManager").color(p.mu));
            ui.horizontal(|ui| {
                c.shot = widgets::primary(ui, &p, "Take screenshot", can_use, 0.0).clicked();
                c.open_shots = widgets::ghost(ui, &p, "Open folder", true, 0.0).clicked();
            });
        });

        // --- installed apps ---
        let filter = self.app_filter.to_lowercase();
        let shown: Vec<String> = self.apps.iter().filter(|a| a.to_lowercase().contains(&filter)).cloned().collect();
        widgets::panel(ui, &p, |ui| {
            ui.label(widgets::bold("Installed apps", 16.0));
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut self.app_filter).hint_text("Filter").desired_width(220.0));
                c.system_changed = ui.checkbox(&mut self.show_system, "Show system apps").changed();
            });
            egui::Frame::none()
                .stroke(Stroke::new(1.0_f32, p.ln))
                .rounding(Rounding::same(12.0))
                .inner_margin(Margin::same(6.0))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    egui::ScrollArea::vertical().id_salt("apps_list").max_height(170.0).auto_shrink([false, true]).show(
                        ui,
                        |ui| {
                            if shown.is_empty() {
                                let t = if self.adb_connected { "No apps to show." } else { "Connect an emulator to see its apps." };
                                ui.label(RichText::new(t).color(p.mu));
                            }
                            for pkg in &shown {
                                let selected = self.sel_app.as_deref() == Some(pkg.as_str());
                                if ui.selectable_label(selected, pkg).clicked() {
                                    self.sel_app = Some(pkg.clone());
                                }
                            }
                        },
                    );
                });
            c.uninstall = widgets::danger(ui, &p, "Uninstall selected", can_use, 0.0).clicked();
        });

        // --- make it faster ---
        widgets::panel(ui, &p, |ui| {
            ui.label(widgets::bold("Make the running device faster", 16.0));
            ui.label(RichText::new("Changes the emulator only, never your PC. Most of it can be undone.").color(p.mu));
            ui.checkbox(&mut self.opt.animations_off, "Turn off animations (the interface feels much snappier)");
            ui.checkbox(&mut self.opt.finish_dexopt, "Finish background app optimization now (can take a few minutes)");
            ui.checkbox(&mut self.opt.free_caches, "Free cached files");
            ui.checkbox(
                &mut self.opt.disable_google_apps,
                "Turn off optional Google apps (Maps, YouTube, Photos, Gmail, Drive, Meet, Music, Movies)",
            );
            ui.checkbox(&mut self.opt.skip_app_scan, "Skip Play Protect scanning of installed apps");
            ui.horizontal(|ui| {
                c.optimize = widgets::primary(ui, &p, "Apply", can_use, 96.0).clicked();
                c.restore = widgets::ghost(ui, &p, "Undo the ticked items", can_use, 0.0).clicked();
            });
        });

        // --- run a command ---
        widgets::panel(ui, &p, |ui| {
            ui.label(widgets::bold("Run a command (adb shell)", 16.0));
            ui.horizontal(|ui| {
                let w = (ui.available_width() - 120.0).max(120.0);
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut self.shell_cmd)
                        .hint_text("e.g. getprop ro.build.version.release")
                        .desired_width(w),
                );
                let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                let clicked = widgets::primary(ui, &p, "Run", can_use, 96.0).clicked();
                c.run_shell = clicked || (enter && can_use);
            });
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Examples:").color(p.mu));
                for (label, cmd) in SHELL_EXAMPLES {
                    if widgets::chip_button(ui, &p, label).clicked() {
                        self.shell_cmd = cmd.to_string();
                    }
                }
            });
            ui.label(RichText::new("The output appears in the log below. Commands run on the emulator only.").color(p.mu).small());
        });

        // --- copy files ---
        widgets::panel(ui, &p, |ui| {
            ui.label(widgets::bold("Copy files", 16.0));
            ui.label(RichText::new("PC to emulator (you can also drag any file onto this window):").color(p.mu));
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut self.push_src).hint_text(r"C:\path\to\file").desired_width(240.0));
                ui.label("to");
                ui.add(egui::TextEdit::singleline(&mut self.push_dest).desired_width(150.0));
                c.push = widgets::primary(ui, &p, "Copy to emulator", can_use, 0.0).clicked();
            });
            ui.label(RichText::new("Emulator to PC:").color(p.mu));
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.pull_src)
                        .hint_text("/sdcard/Download/file.txt")
                        .desired_width(240.0),
                );
                c.pull = widgets::primary(ui, &p, "Copy to PC", can_use, 0.0).clicked();
                c.open_pulled = widgets::ghost(ui, &p, "Open folder", true, 0.0).clicked();
            });
            ui.label(RichText::new("Files copied to the PC go to Downloads\\AndroidEmulatorManager.").color(p.mu).small());
        });

        // --- act on clicks ---
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
        if c.optimize {
            self.on_optimize(false);
        }
        if c.restore {
            self.on_optimize(true);
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
}

// ---------- Keymap ----------

/// A key binding tile. Returns (test clicked, remove clicked).
fn keycap_tile(ui: &mut egui::Ui, p: &Pal, key: &str, pos: &str, can_test: bool, editing: bool) -> (bool, bool) {
    let (mut test, mut remove) = (false, false);
    Frame::none()
        .stroke(Stroke::new(1.0_f32, p.ln))
        .rounding(Rounding::same(14.0))
        .inner_margin(Margin::same(10.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                Frame::none()
                    .fill(p.bg)
                    .stroke(Stroke::new(1.0_f32, p.ln2))
                    .rounding(Rounding::same(10.0))
                    .inner_margin(Margin::symmetric(12.0, 10.0))
                    .show(ui, |ui| {
                        ui.set_min_width(34.0);
                        ui.label(widgets::bold(key, 14.0));
                    });
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    ui.label(RichText::new(pos).color(p.mu));
                    ui.horizontal(|ui| {
                        ui.add_enabled_ui(can_test, |ui| {
                            test = widgets::link(ui, p, "Test tap").clicked();
                        });
                        ui.add_enabled_ui(editing, |ui| {
                            remove = widgets::link(ui, p, "Remove").clicked();
                        });
                    });
                });
            });
        });
    (test, remove)
}

fn marker(painter: &egui::Painter, pos: egui::Pos2, label: &str) {
    painter.circle(pos, 16.0, egui::Color32::WHITE, Stroke::new(1.0_f32, egui::Color32::from_black_alpha(60)));
    let short: String = label.chars().take(3).collect();
    painter.text(
        pos,
        egui::Align2::CENTER_CENTER,
        short,
        egui::FontId::new(10.5, theme::bold_family()),
        egui::Color32::from_rgb(0x34, 0x34, 0xC8),
    );
}

impl App {
    pub(super) fn ui_keymap(&mut self, ui: &mut egui::Ui) {
        let p = theme::pal(ui);
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
                        if let Some(prof) = self.km.as_mut() {
                            prof.set(label, fx, fy);
                        }
                        self.km_pending = None;
                        k.save = true;
                    } else {
                        self.info = Some("That key can't be used. Try a letter, number, arrow key, Space, or Enter.".to_string());
                    }
                }
            }
        }

        widgets::page_header(ui, &p, "Keymap", "Play with your keyboard (experimental)", |ui| {
            if !self.adb_ok {
                widgets::chip(ui, &p, Kind::Warn, "Set up the SDK first");
            } else if !self.adb_connected {
                widgets::chip(ui, &p, Kind::Warn, "No running emulator");
            } else if enabled && self.km_active {
                widgets::chip(ui, &p, Kind::Ok, &format!("Mapping on · {} pauses", keymap::TOGGLE_LABEL));
            } else if enabled {
                widgets::chip(ui, &p, Kind::Warn, &format!("Paused · {} resumes", keymap::TOGGLE_LABEL));
            } else {
                widgets::chip(ui, &p, Kind::Stopped, "Mapping off");
            }
        });
        ui.label(
            RichText::new(
                "Taps arrive about 0.1-0.3 s late and one spot at a time (no joystick). While mapping is on, mapped keys do not reach the emulator.",
            )
            .color(p.mu)
            .small(),
        );

        let cur_name = self.km.as_ref().map(|prof| prof.name.clone());
        let editing = !enabled;

        ui.horizontal_top(|ui| {
            // ----- left: the phone -----
            ui.vertical(|ui| {
                ui.set_width(300.0);
                widgets::panel(ui, &p, |ui| {
                    let (dw, dh) = self.km_dev_size.unwrap_or((1080, 1920));
                    let scale = (250.0 / dw as f32).min(400.0 / dh as f32);
                    let size = egui::vec2(dw as f32 * scale, dh as f32 * scale);
                    ui.vertical_centered(|ui| {
                        let rect = match (&self.km_tex, self.km_dev_size) {
                            (Some(tex), Some(_)) => {
                                let resp = ui.add(
                                    egui::Image::new((tex.id(), size)).rounding(Rounding::same(18.0)).sense(Sense::click()),
                                );
                                if editing && resp.clicked() {
                                    if let Some(pos) = resp.interact_pointer_pos() {
                                        let fx = ((pos.x - resp.rect.min.x) / size.x).clamp(0.0, 1.0);
                                        let fy = ((pos.y - resp.rect.min.y) / size.y).clamp(0.0, 1.0);
                                        k.clicked_at = Some((fx, fy));
                                    }
                                }
                                resp.rect
                            }
                            _ => {
                                let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
                                ui.painter().rect_filled(rect, Rounding::same(22.0), p.g1);
                                ui.painter().text(
                                    rect.center(),
                                    egui::Align2::CENTER_CENTER,
                                    "Press \"Capture screen\"\nwhile your game is running",
                                    egui::FontId::new(13.0, theme::bold_family()),
                                    p.on,
                                );
                                rect
                            }
                        };
                        let painter = ui.painter_at(rect.expand(8.0));
                        painter.rect_stroke(rect, Rounding::same(18.0), Stroke::new(4.0_f32, p.pf));
                        if let Some(prof) = &self.km {
                            for b in &prof.binds {
                                marker(&painter, rect.min + egui::vec2(b.x * size.x, b.y * size.y), &b.key);
                            }
                        }
                        if let Some((px, py)) = self.km_pending {
                            let pos = rect.min + egui::vec2(px * size.x, py * size.y);
                            painter.circle_stroke(pos, 20.0, Stroke::new(2.5_f32, p.wn));
                        }
                    });
                });
            });

            // ----- right: profile, keys, actions -----
            ui.vertical(|ui| {
                ui.set_width(ui.available_width());
                widgets::panel(ui, &p, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(widgets::bold("Profile", 14.0));
                        ui.add_enabled_ui(editing, |ui| {
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
                                .hint_text("New profile (game name)")
                                .desired_width(150.0),
                        );
                        k.create = widgets::ghost(ui, &p, "Create", editing, 0.0).clicked();
                        k.delete = widgets::danger(ui, &p, "Delete", cur_name.is_some() && editing, 0.0).clicked();
                    });

                    if self.km.is_none() {
                        ui.label(RichText::new("Create a profile first (one per game works best), then capture the screen.").color(p.mu));
                    } else if self.km_pending.is_some() {
                        ui.label(widgets::bold("Now press the key for the spot you clicked (Esc cancels).", 13.0).color(p.wn));
                    } else if editing {
                        ui.label(RichText::new("Click a spot on the screen, then press a key.").color(p.mu));
                    } else {
                        ui.label(RichText::new("Turn mapping off to edit.").color(p.mu));
                    }

                    if let Some(prof) = &self.km {
                        if prof.binds.is_empty() {
                            ui.label(RichText::new("No keys yet.").color(p.mu));
                        }
                        let can_test = can_use && !enabled && self.km_dev_size.is_some();
                        ui.columns(2, |cols| {
                            for (i, b) in prof.binds.iter().enumerate() {
                                let pos = format!("{:.0}%, {:.0}%", b.x * 100.0, b.y * 100.0);
                                let (t, r) = keycap_tile(&mut cols[i % 2], &p, &b.key, &pos, can_test, editing);
                                if t {
                                    k.test = Some(i);
                                }
                                if r {
                                    k.remove = Some(i);
                                }
                            }
                        });
                    }

                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        k.capture = widgets::ghost(ui, &p, "Capture screen", can_use && !enabled, 0.0).clicked();
                        k.refresh = widgets::ghost(ui, &p, "Refresh", !self.busy, 0.0).clicked();
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let can_toggle = self.adb_ok
                                && !self.km_starting
                                && (enabled || self.km.as_ref().map_or(false, |prof| !prof.binds.is_empty()));
                            let label = if enabled { "Turn mapping off" } else { "Turn mapping on" };
                            k.toggle = widgets::primary(ui, &p, label, can_toggle, 0.0).clicked();
                            if enabled {
                                let pl = if self.km_active { "Pause" } else { "Resume" };
                                k.pause = widgets::ghost(ui, &p, pl, true, 0.0).clicked();
                            }
                        });
                    });
                });
            });
        });

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
            if let Some(prof) = self.km.as_mut() {
                if i < prof.binds.len() {
                    prof.binds.remove(i);
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
}
