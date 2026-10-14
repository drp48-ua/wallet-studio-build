use std::collections::HashMap;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::thread;
use std::time::Instant;

use eframe::egui;
use image::DynamicImage;

use crate::apple;
use crate::card_gallery;
use crate::device::{ConnectionMode, DeviceInfo, DeviceTransport, list_connected_devices};
use crate::flasher::{flash_passcode_theme, replace_wallet_skin, restore_wallet_original};
use crate::image_skin::{CardCrop, PreparedSkin};
use crate::i18n::Language;
use crate::passthm::{PasscodeTheme, parse_passthm_file};
use crate::scanner::{SavedCard, load_saved_cards, scan_syslog_for_cards};
#[path = "ui_card_state.rs"]
mod ui_card_state;
use ui_card_state::{AutoCapture, CapturePhase, OriginalState, SnapshotCache, capture_device_for_selection};

struct CardSnapshot {
    udid: Option<String>, known_device: Option<String>, original: OriginalState,
    restore: bool, phase: Result<Option<String>, ()>,
    original_image: Option<egui::ColorImage>, applied_image: Option<egui::ColorImage>,
}
enum CardCacheMessage {
    Card { epoch: u64, hash: String, snapshot: CardSnapshot },
    Finished { epoch: u64, recovery: Result<Vec<crate::wallet_export::PendingRecovery>, String> },
}

fn thumbnail(bytes: &[u8]) -> Option<egui::ColorImage> {
    let image = crate::design_library::decode_still(bytes).ok()?.thumbnail(500,320).to_rgba8();
    Some(egui::ColorImage::from_rgba_unmultiplied([image.width() as usize,image.height() as usize],image.as_raw()))
}

/// All persistence checks and decoding happen on the snapshot worker, never paint.
fn load_card_snapshot(selected: Option<&str>, hash: &str) -> CardSnapshot {
    let known_device = crate::bank_backup::verified_baseline_device_for_card(hash);
    let udid = selected.map(str::to_string).or_else(|| known_device.clone());
    let mut result = CardSnapshot { udid: udid.clone(), known_device,
        original: OriginalState::Missing, restore: false, phase: Ok(None),
        original_image: None, applied_image: None };
    let Some(udid) = udid else { return result; };
    let bank = crate::bank_backup::load_baseline(&udid,hash);
    let mut bank_verified = false;
    match bank {
        Ok(Some(baseline)) => {
            bank_verified = true; result.original = OriginalState::Verified;
            result.original_image = thumbnail(&baseline.remote_artwork_bytes);
        }
        Err(_) => result.original = OriginalState::Blocked,
        Ok(None) => if crate::wallet_backup::backup_evidence_exists(&udid,hash) {
            match crate::wallet_backup::load_original_assets(&udid,hash) {
                Ok(assets) => { result.original = OriginalState::Verified;
                    result.original_image = assets.iter().filter(|(leaf,_)| leaf.ends_with(".png"))
                        .find_map(|(_,bytes)| thumbnail(bytes)); }
                Err(_) => result.original = OriginalState::Blocked,
            }
        },
    }
    match crate::bank_changes::load_latest_intent(&udid,hash) {
        Ok(intent) => {
            result.phase = Ok(intent.as_ref().map(|intent| intent.phase.clone()));
            if intent.is_some() && result.original == OriginalState::Missing { result.original = OriginalState::Blocked; }
            result.restore = result.original == OriginalState::Verified && (!bank_verified
                || intent.as_ref().is_some_and(|intent| intent.phase != "restored"));
            if let Some(intent) = intent.filter(|intent| intent.phase == "applied") {
                result.applied_image = intent.assets.get("cardBackgroundCombined@3x.png").and_then(|bytes| thumbnail(bytes));
                if result.original == OriginalState::Missing { result.original = OriginalState::Blocked; }
            }
        }
        Err(_) => { result.phase = Err(()); if result.original == OriginalState::Missing { result.original = OriginalState::Blocked; } }
    }
    result
}

#[derive(PartialEq, Eq)]
enum AppTab {
    Wallet,
    Passcode,
    Help,
}

enum BackgroundTaskMessage {
    Progress { step: usize, total: usize, message: String },
    Log(String),
    CardFound { hash: String, name: String },
    DesignSaved(PathBuf),
    Done(Result<String, String>),
    OriginalBackupDone { udid: String, hash: String, result: Result<Option<String>, String> },
}

fn cached_bank_apply_plan(phase: &Result<Option<String>, ()>) -> crate::flasher::BankApplyPlan {
    match phase {
        Ok(phase) => crate::flasher::bank_apply_plan(phase.as_deref()),
        Err(()) => crate::flasher::BankApplyPlan::Block,
    }
}

fn redact_log_text(text: &str, mut identifiers: Vec<String>) -> String {
    identifiers.retain(|value| !value.is_empty());
    identifiers.sort_by_key(|value| std::cmp::Reverse(value.len()));
    identifiers.dedup();
    if identifiers.is_empty() { return text.to_string(); }
    let pattern = identifiers.iter().map(|value| regex::escape(value)).collect::<Vec<_>>().join("|");
    match regex::RegexBuilder::new(&pattern).case_insensitive(true).build() {
        Ok(regex) => regex.replace_all(text, "[dato oculto]").into_owned(),
        Err(_) => "No se pudo preparar una exportación protegida del registro.".into(),
    }
}

#[cfg(windows)]
fn current_timestamp() -> String {
    #[repr(C)]
    struct SystemTime {
        w_year: u16,
        w_month: u16,
        w_day_of_week: u16,
        w_day: u16,
        w_hour: u16,
        w_minute: u16,
        w_second: u16,
        w_milliseconds: u16,
    }
    unsafe extern "system" {
        fn GetLocalTime(lpSystemTime: *mut SystemTime);
    }
    let mut st = std::mem::MaybeUninit::<SystemTime>::uninit();
    unsafe {
        GetLocalTime(st.as_mut_ptr());
        let st = st.assume_init();
        format!(
            "{:02}:{:02}:{:02}.{:03}",
            st.w_hour, st.w_minute, st.w_second, st.w_milliseconds
        )
    }
}

#[cfg(not(windows))]
fn current_timestamp() -> String {
    "00:00:00.000".to_string()
}

pub struct AirCardApp {
    brand_texture: egui::TextureHandle,
    current_tab: AppTab,
    language: Language,
    apple_status: String,
    apple_ready: bool,

    // Device management
    devices: Vec<DeviceInfo>,
    selected_udid: Option<String>,
    connection_mode: ConnectionMode,

    // Wallet tab
    card_hash: String,
    saved_cards: Vec<SavedCard>,
    gallery_open: bool,
    card_name_draft: String,
    rename_error: Option<String>,
    original_thumbnails: HashMap<String, egui::TextureHandle>,
    applied_thumbnails: HashMap<String, egui::TextureHandle>,
    card_snapshots: SnapshotCache<CardSnapshot>,
    card_cache_rx: Option<Receiver<CardCacheMessage>>,
    card_cache_refresh: bool,
    card_cache_generation: Arc<AtomicU64>,
    auto_capture: Option<AutoCapture>,
    offline_baseline_device: HashMap<String, String>,
    gif_frames: Vec<crate::animated_input::GifFrame>,
    gif_frame: usize,
    gif_playback: Option<(Instant, usize)>,
    source_path: Option<PathBuf>,
    library_designs: Vec<crate::design_library::Design>,
    library_selected: Option<PathBuf>,
    library_error: Option<String>,
    source_image: Option<DynamicImage>,
    source_texture: Option<egui::TextureHandle>,
    crop_focus: [f32; 2],
    crop_zoom: f32,
    crop_edges: Option<[f32; 4]>,
    crop_draft: Option<CardCrop>,
    crop_dirty: bool,
    skin: Option<PreparedSkin>,
    scanning_syslog: bool,
    scan_stop_flag: Option<Arc<AtomicBool>>,

    // Passcode tab
    theme_path: Option<PathBuf>,
    loaded_theme: Option<PasscodeTheme>,
    forced_telephony_ver: String,
    keypad_language: String,
    passcode_bold: bool,
    keypad_textures: Vec<(String, egui::TextureHandle)>,

    // Worker thread & progress
    is_busy: bool,
    progress_step: usize,
    progress_total: usize,
    progress_msg: String,
    status_msg: String,
    task_rx: Option<Receiver<BackgroundTaskMessage>>,
    logs: Vec<String>,
    show_logs_window: bool,
    pending_recoveries: Vec<crate::wallet_export::PendingRecovery>,
    recovery_status_error: Option<String>,
}

impl AirCardApp {
    fn current_crop(&self) -> CardCrop {
        CardCrop { focus: self.crop_focus, zoom: self.crop_zoom, edges: self.crop_edges }
    }

    fn show_crop_editor(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.crop_draft else { return; };
        let (Some(source), Some(texture)) = (self.source_image.as_ref(), self.source_texture.as_ref()) else {
            self.crop_draft = None;
            return;
        };
        let dimensions = [source.width(), source.height()];
        let texture = texture.clone();
        let mut open = true;
        let mut accept = false;
        let mut cancel = false;
        egui::Window::new("Recortar imagen")
            .open(&mut open).collapsible(false).default_width(620.0)
            .max_width((ctx.content_rect().width() - 48.0).max(200.0))
            .max_height((ctx.content_rect().height() - 48.0).max(300.0))
            .show(ctx, |ui| {
                ui.label("Arrastra cada lado por separado o ajusta su porcentaje. Las esquinas cambian solo sus dos lados.");
                egui::ScrollArea::vertical().show(ui, |ui| {
                    crate::crop_editor::selection(ui, &texture, dimensions, &mut draft,
                        (ctx.content_rect().height() - 280.0).clamp(100.0, 400.0));
                    crate::crop_editor::edge_controls(ui, dimensions, &mut draft);
                    ui.label("La imagen original se conserva. El área elegida se ajusta al tamaño de tarjeta sin recortes adicionales.");
                });
                ui.separator();
                ui.horizontal_wrapped(|ui| {
                    accept = ui.add_enabled(!self.is_busy, egui::Button::new("Aplicar recorte")).clicked();
                    cancel = ui.button("Cancelar").clicked();
                    if ui.button("Restablecer recorte").clicked() { draft = CardCrop::full_image(); }
                });
            });
        if !open || cancel { self.crop_draft = None; }
        else if accept {
            // Prepare first: cancellation or a failed encode never changes the committed design.
            match PreparedSkin::from_image_with_crop(self.source_image.as_ref().unwrap().clone(), draft) {
                Ok(skin) => {
                    self.crop_focus = draft.focus;
                    self.crop_zoom = draft.zoom;
                    self.crop_edges = draft.edges;
                    self.skin = Some(skin);
                    self.crop_dirty = false;
                    self.crop_draft = None;
                    self.status_msg = "Recorte preparado. Puedes exportarlo o aplicarlo a la tarjeta.".into();
                }
                Err(error) => {
                    self.add_log(format!("Image crop failed: {error:#}"));
                    self.status_msg = "No se pudo preparar el recorte. El diseño anterior se conserva.".into();
                    self.crop_draft = Some(draft);
                }
            }
        } else { self.crop_draft = Some(draft); }
    }

    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        setup_custom_fonts(&cc.egui_ctx);
        setup_custom_theme(&cc.egui_ctx);
        let brand_image = image::load_from_memory(include_bytes!("../assets/wallet-studio.png"))
            .expect("bundled Wallet Studio logo is a valid image")
            .to_rgba8();
        let brand_size = [brand_image.width() as usize, brand_image.height() as usize];
        let brand_texture = cc.egui_ctx.load_texture(
            "wallet-studio-brand",
            egui::ColorImage::from_rgba_unmultiplied(brand_size, brand_image.as_raw()),
            egui::TextureOptions::LINEAR,
        );

        let (apple_ready, apple_status) = match apple::verify_support() {
            Ok(msg) => (true, msg),
            Err(err) => (false, err.to_string()),
        };

        let language = Language::load();
        let saved_cards = load_saved_cards();
        let offline_baseline_device = HashMap::new();
        let mut app = Self {
            brand_texture,
            current_tab: AppTab::Wallet,
            language,
            apple_status,
            apple_ready,

            devices: Vec::new(),
            selected_udid: None,
            connection_mode: ConnectionMode::Auto,

            card_hash: String::new(),
            saved_cards,
            gallery_open: true,
            card_name_draft: String::new(),
            rename_error: None,
            original_thumbnails: HashMap::new(),
            applied_thumbnails: HashMap::new(),
            card_snapshots: SnapshotCache::default(), card_cache_rx: None,
            card_cache_refresh: true, auto_capture: None,
            card_cache_generation: Arc::new(AtomicU64::new(0)),
            offline_baseline_device,
            gif_frames: Vec::new(),
            gif_frame: 0,
            gif_playback: None,
            source_path: None,
            library_designs: Vec::new(),
            library_selected: None,
            library_error: None,
            source_image: None,
            source_texture: None,
            crop_focus: [0.5, 0.5],
            crop_zoom: 1.0,
            crop_edges: None,
            crop_draft: None,
            crop_dirty: false,
            skin: None,
            scanning_syslog: false,
            scan_stop_flag: None,

            theme_path: None,
            loaded_theme: None,
            forced_telephony_ver: "Auto (TelephonyUI-10)".to_string(),
            keypad_language: "English".to_string(),
            passcode_bold: false,
            keypad_textures: Vec::new(),

            is_busy: false,
            progress_step: 0,
            progress_total: 0,
            progress_msg: String::new(),
            status_msg: language
                .text("Ready. Connect iPhone via USB or paired WiFi and unlock it.")
                .to_string(),
            task_rx: None,
            logs: Vec::new(),
            show_logs_window: false,
            pending_recoveries: Vec::new(),
            recovery_status_error: Some("Comprobando el estado local de recuperación…".into()),
        };

        app.add_log("Wallet Studio initialized");
        app.add_log(format!("Apple Support Runtime: {}", if app.apple_ready { "Loaded and operational" } else { "Not found (iTunes required)" }));
        app.add_log(format!("Loaded {} saved card(s) from database", app.saved_cards.len()));

        if app.apple_ready {
            app.refresh_devices();
        }
        app.refresh_recovery_status();
        app.refresh_design_library();

        app
    }

    fn add_log(&mut self, text: impl AsRef<str>) {
        let ts = current_timestamp();
        self.logs.push(format!("[{}] {}", ts, text.as_ref()));
        if self.logs.len() > 1000 {
            self.logs.remove(0);
        }
    }

    fn redacted_logs(&self) -> String {
        let mut identifiers = Vec::new();
        for device in &self.devices {
            identifiers.extend([device.udid.clone(), device.name.clone()]);
        }
        for card in &self.saved_cards {
            identifiers.extend([card.hash.clone(), card.name.clone()]);
        }
        identifiers.extend(self.selected_udid.iter().cloned());
        identifiers.push(self.card_hash.trim().to_string());
        identifiers.extend(self.offline_baseline_device.values().cloned());
        for entry in &self.pending_recoveries {
            identifiers.extend([entry.udid.clone(), entry.card_hash.clone()]);
        }
        if let Some(home) = std::env::var_os("USERPROFILE") {
            let home = home.to_string_lossy().to_string();
            identifiers.extend([home.replace('\\', "/"), home]);
        }
        format!("Wallet Studio · registro con datos conocidos ocultos. Revisa el contenido antes de compartir: pueden quedar datos privados.\n{}",
            redact_log_text(&self.logs.join("\n"), identifiers))
    }

    fn refresh_devices(&mut self) {
        self.add_log("Scanning for connected iOS devices via usbmuxd...");
        match list_connected_devices() {
            Ok(devs) => {
                self.devices = devs;
                let selection_still_exists = self.selected_udid.as_ref().is_some_and(|selected| {
                    self.devices
                        .iter()
                        .any(|device| device.udid.eq_ignore_ascii_case(selected))
                });
                if !selection_still_exists && !self.devices.is_empty() {
                    self.selected_udid = Some(self.devices[0].udid.clone());
                }
                if self.devices.is_empty() {
                    self.selected_udid = None;
                    self.add_log("No devices detected. Connect by USB, or enable WiFi sync after initial USB pairing.");
                    self.status_msg = self
                        .language
                        .text("No iPhone connected via USB or paired WiFi.")
                        .to_string();
                } else {
                    let dev_logs: Vec<String> = self.devices.iter().enumerate().map(|(i, d)| {
                        format!("Device #{}: {} - UDID: {}", i + 1, d, d.udid)
                    }).collect();
                    for line in dev_logs {
                        self.add_log(line);
                    }
                    self.status_msg = format!(
                        "{} {} {} {}",
                        self.language.text("Found"),
                        self.devices.len(),
                        self.language.text("connected device(s); transport mode:"),
                        self.language.text(self.connection_mode.label())
                    );
                }
            }
            Err(err) => {
                self.add_log(format!("Device scan error: {}", err));
                self.status_msg = format!(
                    "{} {}",
                    self.language.text("Could not enumerate devices:"),
                    err
                );
            }
        }
        self.invalidate_card_cache();
    }

    fn selected_transport_available(&self) -> bool {
        self.selected_udid.as_ref().is_some_and(|selected| {
            self.devices.iter().any(|device| {
                if !device.udid.eq_ignore_ascii_case(selected) {
                    return false;
                }
                if self.connection_mode == ConnectionMode::Wifi {
                    return device.has_transport(DeviceTransport::Wifi)
                        && !device.has_transport(DeviceTransport::Usb);
                }
                device.supports(self.connection_mode)
            })
        })
    }

    fn validate_selected_transport(&mut self, operation: &str) -> bool {
        if self.selected_udid.is_none() {
            self.add_log(format!("{} failed: No connected iPhone selected.", operation));
            self.status_msg = self.language.text("Please select a connected iPhone.").to_string();
            return false;
        }
        if !self.selected_transport_available() {
            let wifi_has_usb_attached = self.connection_mode == ConnectionMode::Wifi
                && self.selected_udid.as_ref().is_some_and(|selected| {
                    self.devices.iter().any(|device| {
                        device.udid.eq_ignore_ascii_case(selected)
                            && device.has_transport(DeviceTransport::Wifi)
                            && device.has_transport(DeviceTransport::Usb)
                    })
                });
            self.add_log(format!(
                "{} failed: Selected device is unavailable in {} mode.",
                operation,
                self.connection_mode.label()
            ));
            self.status_msg = if wifi_has_usb_attached {
                self.language
                    .text("Disconnect the USB cable and refresh to guarantee the full AirTraffic path uses WiFi.")
                    .to_string()
            } else {
                format!(
                    "{} {} {}",
                    self.language.text("Selected iPhone has no"),
                    self.language.text(self.connection_mode.label()),
                    self.language.text("connection. Refresh devices or change transport mode.")
                )
            };
            return false;
        }
        true
    }

    fn select_skin(&mut self, ctx: &egui::Context) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Imágenes y GIF", &["png", "jpg", "jpeg", "webp", "gif"])
            .pick_file()
        else {
            return;
        };

        self.load_skin_path(ctx, path);
    }

    fn refresh_design_library(&mut self) {
        match crate::design_library::library_dir().and_then(|root| crate::design_library::discover(&root)) {
            Ok(designs) => {
                if !designs.iter().any(|design| Some(&design.path) == self.library_selected.as_ref()) {
                    self.library_selected = designs.first().map(|design| design.path.clone());
                }
                self.library_designs = designs;
                self.library_error = None;
            }
            Err(_) => {
                self.library_designs.clear();
                self.library_selected = None;
                self.library_error = Some("No se pudo abrir designs. Revisa la carpeta junto a Wallet Studio.exe.".into());
            }
        }
    }

    fn load_skin_path(&mut self, ctx: &egui::Context, path: PathBuf) {
        if self.is_busy { return; }
        let decoded = crate::design_library::read_image(&path).and_then(|bytes| {
            if path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("gif")) {
                let frames = crate::animated_input::decode_gif(Cursor::new(bytes))?;
                let first = frames.first().ok_or_else(|| anyhow::anyhow!("El GIF no tiene fotogramas"))?;
                Ok((first.image.clone(), frames))
            } else {
                crate::design_library::decode_still(&bytes).map(|image| (image, Vec::new()))
            }
        });
        match decoded {
            Ok((source_image, frames)) => {
                let skin = match PreparedSkin::from_image_with_focus(
                    source_image.clone(),
                    0.5,
                    0.5,
                ) {
                    Ok(skin) => skin,
                    Err(error) => {
                        self.add_log(format!("Image preparation failed: {error:#}"));
                        self.status_msg = self.language.text("No se pudo preparar la imagen. Revisa el formato y vuelve a intentarlo.").to_string();
                        return;
                    }
                };
                let source_rgba = source_image.thumbnail(2048, 2048).to_rgba8();
                let source_preview = egui::ColorImage::from_rgba_unmultiplied(
                    [source_rgba.width() as usize, source_rgba.height() as usize],
                    source_rgba.as_raw(),
                );

                self.add_log(format!(
                    "Skin processed: source {}x{} resampled to 1536x969 PNG ({:.1} KB)",
                    skin.source_width,
                    skin.source_height,
                    skin.png.len() as f32 / 1024.0,
                ));
                self.source_texture = Some(ctx.load_texture(
                    "card-skin-source-preview",
                    source_preview,
                    egui::TextureOptions::LINEAR,
                ));
                self.status_msg = self.language.text("Diseño preparado.").to_string();
                self.source_path = Some(path);
                self.gif_frames = frames;
                self.gif_frame = 0;
                self.gif_playback = None;
                self.source_image = Some(source_image);
                self.crop_focus = [0.5, 0.5];
                self.crop_zoom = 1.0;
                self.crop_edges = None;
                self.crop_draft = None;
                self.crop_dirty = false;
                self.skin = Some(skin);
            }
            Err(error) => {
                self.add_log(format!("Image decode failed: {error:#}"));
                self.status_msg = "No se pudo abrir el diseño. Usa PNG, JPG o WebP de hasta 16 MP, o GIF de hasta 120 fotogramas; máximo 32 MB. El diseño anterior se conserva.".into();
            }
        }
    }

    fn update_gif_texture(&mut self, ctx: &egui::Context, image: &DynamicImage) {
        let rgba = image.thumbnail(2048, 2048).to_rgba8();
        let preview = egui::ColorImage::from_rgba_unmultiplied(
            [rgba.width() as usize, rgba.height() as usize], rgba.as_raw());
        if let Some(texture) = self.source_texture.as_mut() {
            texture.set(preview, egui::TextureOptions::LINEAR);
        } else {
            self.source_texture = Some(ctx.load_texture("gif-frame-preview", preview, egui::TextureOptions::LINEAR));
        }
    }

    fn tick_gif(&mut self, ctx: &egui::Context) {
        let Some((started, first)) = self.gif_playback else { return; };
        if self.current_tab != AppTab::Wallet || self.is_busy {
            self.freeze_gif(ctx, self.gif_frame);
            return;
        }
        if let Some((index, until_next)) = crate::animated_input::playback_position(&self.gif_frames, first, started.elapsed()) {
            if index != self.gif_frame {
                self.gif_frame = index;
                // Keep only one GPU texture; PNG/PDF generation happens on freeze.
                if let Some(image) = crate::animated_input::selected_frame(&self.gif_frames, index) {
                    self.update_gif_texture(ctx, &image);
                }
            }
            ctx.request_repaint_after(until_next);
        }
    }

    fn freeze_gif(&mut self, ctx: &egui::Context, index: usize) {
        let Some(image) = crate::animated_input::selected_frame(&self.gif_frames, index) else { return; };
        match PreparedSkin::from_image_with_crop(image.clone(), self.current_crop()) {
            Ok(skin) => {
                self.gif_playback = None;
                self.gif_frame = index;
                self.update_gif_texture(ctx, &image);
                self.source_image = Some(image);
                self.skin = Some(skin);
                self.crop_dirty = false;
                self.status_msg = format!("Fotograma {} preparado como imagen estática.", index + 1);
            }
            Err(error) => {
                self.add_log(format!("GIF frame preparation failed: {error:#}"));
                self.status_msg = "No se pudo preparar el fotograma. El diseño anterior se conserva.".into();
                // Restore the previously coherent static preview on a selection failure.
                if self.gif_playback.is_none() {
                    if let Some(image) = self.source_image.clone() { self.update_gif_texture(ctx, &image); }
                }
            }
        }
    }

    fn rebuild_skin_from_source(&mut self) {
        let Some(source_image) = self.source_image.as_ref() else {
            return;
        };

        match PreparedSkin::from_image_with_crop(source_image.clone(), self.current_crop()) {
            Ok(skin) => {
                self.add_log(format!(
                    "Crop updated: focus ({:.2}, {:.2}), prepared PNG {:.1} KB",
                    self.crop_focus[0],
                    self.crop_focus[1],
                    skin.png.len() as f32 / 1024.0,
                ));
                self.skin = Some(skin);
                self.crop_dirty = false;
                self.status_msg = self
                    .language
                    .text("Crop position updated.")
                    .to_string();
            }
            Err(error) => {
                self.add_log(format!("Crop preparation failed: {error:#}"));
                self.status_msg = self.language.text("No se pudo actualizar el encuadre. Revisa el registro para ver detalles.").to_string();
            }
        }
    }

    fn save_prepared_png(&mut self) {
        if self.gif_playback.is_some() || self.crop_draft.is_some() || self.crop_dirty { return; }
        let Some(skin) = &self.skin else {
            return;
        };
        let Some(path) = rfd::FileDialog::new()
            .set_file_name("wallet-studio-skin.png")
            .save_file()
        else {
            return;
        };
        match std::fs::write(&path, &skin.png) {
            Ok(()) => {
                self.add_log(format!("Exported prepared card skin PNG: {}", path.display()));
                self.status_msg = format!("{} {}", self.language.text("PNG guardado en:"), path.display());
            }
            Err(err) => {
                self.add_log(format!("Failed to save PNG: {err}"));
                self.status_msg = self.language.text("No se pudo guardar el PNG. Revisa la ubicación y los permisos.").to_string();
            }
        }
    }

    fn toggle_syslog_scan(&mut self) {
        if self.scanning_syslog {
            if let Some(flag) = self.scan_stop_flag.take() {
                flag.store(true, Ordering::Relaxed);
            }
            self.scanning_syslog = false;
            self.add_log("Syslog scanning stopped by user.");
            self.status_msg = self.language.text("Syslog scanning stopped.").to_string();
            return;
        }

        if !self.validate_selected_transport("Syslog scan") {
            return;
        }

        let stop_flag = Arc::new(AtomicBool::new(false));
        self.scan_stop_flag = Some(Arc::clone(&stop_flag));
        self.scanning_syslog = true;
        self.is_busy = true;
        self.add_log("Initiating syslog monitor session...");
        self.status_msg = self
            .language
            .text("Scanning syslog... Open Wallet or tap your card on iPhone.")
            .to_string();

        let (tx, rx) = channel();
        self.task_rx = Some(rx);
        let udid = self.selected_udid.clone();
        let connection_mode = self.connection_mode;
        let language = self.language;

        thread::spawn(move || {
            let tx_card = tx.clone();
            let tx_log = tx.clone();
            let res = scan_syslog_for_cards(
                udid.as_deref(),
                connection_mode,
                stop_flag,
                move |hash, name| {
                    let _ = tx_card.send(BackgroundTaskMessage::CardFound { hash, name });
                },
                move |msg| {
                    let _ = tx_log.send(BackgroundTaskMessage::Log(msg));
                },
            );
            match res {
                Ok(()) => {
                    let _ = tx.send(BackgroundTaskMessage::Done(Ok(
                        language.text("Syslog scan finished").into(),
                    )));
                }
                Err(e) => {
                    let _ = tx.send(BackgroundTaskMessage::Done(Err(e.to_string())));
                }
            }
        });
    }

    fn flash_card(&mut self) {
        if self.gif_playback.is_some() || self.crop_draft.is_some() || self.crop_dirty { return; }
        if !self.validate_selected_transport("Card flash") {
            return;
        }
        let Some(udid) = self.selected_udid.clone() else {
            return;
        };
        let hash = self.card_hash.trim().to_string();
        if hash.is_empty() {
            self.add_log("Flash failed: Target card hash is empty.");
            self.status_msg = self
                .language
                .text("Please enter or scan a target card hash.")
                .to_string();
            return;
        }
        let Some((png_bytes, pdf_bytes)) = self.skin.as_ref()
            .map(|skin| (skin.png.clone(), skin.pdf.clone())) else {
                self.add_log("Flash failed: No skin image prepared.");
                self.status_msg = self.language.text("Please choose a card skin image first.").to_string();
                return;
            };

        if !self.original_verified(&hash) {
            self.status_msg = self.language.text("Falta una copia original verificable; no se aplicará el diseño.").to_string();
            return;
        }
        let intent_phase = self.bank_intent_phase(&udid,&hash);
        let plan = cached_bank_apply_plan(&intent_phase);
        if plan == crate::flasher::BankApplyPlan::Block {
            self.status_msg = self.language.text("Hay un cambio anterior sin resolver. La aplicación queda bloqueada para proteger la copia original.").to_string();
            return;
        }

        if let Some(ref flag) = self.scan_stop_flag {
            flag.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        self.scanning_syslog = false;
        self.is_busy = true;
        self.progress_step = 0;
        self.progress_total = 3;
        self.progress_msg = self.language.text("Preparando el diseño de tarjeta…").to_string();
        self.status_msg = self.language.text("Writing card skin to iPhone...").to_string();
        let connection_mode = self.connection_mode;
        let language = self.language;
        let restore_before_apply = plan == crate::flasher::BankApplyPlan::RestoreThenApply;
        self.progress_total = if restore_before_apply { 6 } else { 3 };
        if restore_before_apply {
            self.progress_msg = self.language.text("Restaurando el diseño anterior antes de aplicar el nuevo…").to_string();
        }
        self.add_log(format!(
            "Starting card skin flash for hash: {} (UDID: {}, transport: {})",
            hash,
            udid,
            connection_mode.label()
        ));

        let (tx, rx) = channel();
        self.task_rx = Some(rx);

        thread::spawn(move || {
            let res = (|| -> anyhow::Result<()> {
                let progress_tx = tx.clone();
                let log_tx = tx.clone();
                let library = crate::design_library::library_dir()?;
                crate::design_library::save_before_apply(&library, &png_bytes, |saved| {
                    let _ = tx.send(BackgroundTaskMessage::DesignSaved(saved.to_path_buf()));
                    replace_wallet_skin(&udid, connection_mode, &hash, &png_bytes, &pdf_bytes,
                    move |step, total, msg| {
                        let _ = progress_tx.send(BackgroundTaskMessage::Progress {
                            step,
                            total,
                            message: msg.to_string(),
                        });
                    }, move |msg| { let _ = log_tx.send(BackgroundTaskMessage::Log(msg.to_string())); })
                })?;
                Ok(())
            })();

            match res {
                Ok(()) => {
                    let _ = tx.send(BackgroundTaskMessage::Done(Ok(
                        language
                            .text("Card skin successfully flashed! Force quit Wallet on iPhone and reopen it.")
                            .into(),
                    )));
                }
                Err(e) => {
                    let _ = tx.send(BackgroundTaskMessage::Done(Err(format!("{:#}", e))));
                }
            }
        });
    }

    fn restore_original_card(&mut self) {
        if !self.validate_selected_transport("Restore original card") {
            return;
        }
        let Some(udid) = self.selected_udid.clone() else {
            return;
        };
        let hash = self.card_hash.trim().to_string();
        if hash.is_empty() {
            self.add_log("Restore failed: Target card hash is empty.");
            self.status_msg = self
                .language
                .text("Please enter or scan a target card hash.")
                .to_string();
            return;
        }
        if !self.snapshot(&hash).is_some_and(|snapshot|snapshot.restore) {
            self.add_log("Restore failed: Original card face backup not found.");
            self.status_msg = self
                .language
                .text("Original card backup not found.")
                .to_string();
            return;
        }

        if let Some(ref flag) = self.scan_stop_flag {
            flag.store(true, Ordering::Relaxed);
        }
        self.scanning_syslog = false;
        self.is_busy = true;
        self.progress_step = 0;
        self.progress_total = 3;
        self.progress_msg = self.language.text("Preparando la restauración de la copia original…").to_string();
        self.status_msg = self
            .language
            .text("Restoring original card face...")
            .to_string();
        let connection_mode = self.connection_mode;
        let language = self.language;
        self.add_log(format!(
            "Starting original card face restore for hash: {} (UDID: {}, transport: {})",
            hash,
            udid,
            connection_mode.label()
        ));

        let (tx, rx) = channel();
        self.task_rx = Some(rx);

        thread::spawn(move || {
            let tx_progress = tx.clone();
            let tx_log = tx.clone();
            let res = restore_wallet_original(
                &udid,
                connection_mode,
                &hash,
                move |step, total, msg| {
                    let _ = tx_progress.send(BackgroundTaskMessage::Progress {
                        step,
                        total,
                        message: msg.to_string(),
                    });
                },
                move |msg| {
                    let _ = tx_log.send(BackgroundTaskMessage::Log(msg.to_string()));
                },
            );

            match res {
                Ok(()) => {
                    let _ = tx.send(BackgroundTaskMessage::Done(Ok(
                        language
                            .text("Original card face restored. Force close Wallet and reopen it.")
                            .into(),
                    )));
                }
                Err(e) => {
                    let _ = tx.send(BackgroundTaskMessage::Done(Err(format!("{:#}", e))));
                }
            }
        });
    }

    fn capture_original_backup(&mut self) {
        if self.is_busy || self.scanning_syslog || !self.pending_recoveries.is_empty()
            || self.recovery_status_error.is_some() || !self.selected_usb_ready() {
            return;
        }
        let Some(udid) = self.selected_udid.clone() else { return };
        let hash = self.card_hash.trim().to_string();
        if hash.is_empty() {
            self.status_msg = self.language.text("Selecciona una tarjeta antes de guardar su copia original.").to_string();
            return;
        }
        self.is_busy = true;
        self.progress_step = 0;
        self.progress_total = 0;
        self.progress_msg.clear();
        self.status_msg = "Guardando automáticamente el original. La captura puede moverlo temporalmente y comprueba su devolución; mantén USB conectado, Wallet y Libros cerrados.".into();
        self.add_log("Starting recoverable original-artwork export with protected readback.");
        let connection_mode = ConnectionMode::Usb;
        let (tx, rx) = channel();
        self.task_rx = Some(rx);
        thread::spawn(move || {
            let log_tx = tx.clone();
            let result = (|| -> anyhow::Result<Option<String>> {
                // Revalidate persistence, identity and recovery immediately before
                // any device capture; presentation snapshots never authorize writes.
                if crate::bank_backup::load_baseline(&udid,&hash)?.is_some() { return Ok(Some("verified".into())); }
                if crate::wallet_backup::backup_evidence_exists(&udid,&hash) {
                    crate::wallet_backup::load_original_assets(&udid,&hash)?;
                    return Ok(Some("verified".into()));
                }
                anyhow::ensure!(crate::wallet_export::pending_recovery_operations()?.is_empty(), "Resuelve la recuperación pendiente antes de capturar");
                anyhow::ensure!(crate::bank_changes::load_latest_intent(&udid,&hash)?.is_none(),
                    "Existe historial de diseños sin original verificable; conserva la evidencia y resuelve la recuperación");
                anyhow::ensure!(list_connected_devices()?.iter().any(|device| device.udid == udid && device.has_transport(DeviceTransport::Usb)),
                    "El iPhone seleccionado ya no está conectado por USB; no se inició la captura");
                crate::bank_runtime::capture_baseline(&udid, connection_mode, &hash, move |line| {
                    let _ = log_tx.send(BackgroundTaskMessage::Log(line.to_string()));
                })?;
                Ok(Some("verified".into()))
            })().map_err(|error| format!("{error:#}"));
            let _ = tx.send(BackgroundTaskMessage::OriginalBackupDone { udid, hash, result });
        });
    }

    fn refresh_recovery_status(&mut self) {
        self.invalidate_card_cache();
    }

    fn recover_pending_original(&mut self, operation_dir: PathBuf) {
        if !self.validate_selected_transport("Recuperar original pendiente") { return; }
        let Some(udid) = self.selected_udid.clone() else { return };
        let mode = self.connection_mode;
        self.is_busy = true;
        self.status_msg = "Recuperando el original guardado y comprobando su devolución…".into();
        let (tx, rx) = channel();
        self.task_rx = Some(rx);
        thread::spawn(move || {
            let log_tx = tx.clone();
            let result = crate::wallet_export::resume_pending_return(&operation_dir, &udid, mode, move |line| {
                let _ = log_tx.send(BackgroundTaskMessage::Log(line.into()));
            }).map(|_| "Original recuperado; devolución y archivos de Libros verificados.".to_string())
                .map_err(|error| format!("{error:#}"));
            let _ = tx.send(BackgroundTaskMessage::Done(result));
        });
    }

    fn select_theme_file(&mut self, ctx: &egui::Context) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Passcode Theme", &["passthm", "passtheme", "zip"])
            .pick_file()
        else {
            return;
        };

        self.load_theme_from_path(ctx, &path);
    }

    fn load_theme_from_path(&mut self, ctx: &egui::Context, path: &Path) {
        self.add_log(format!("Opening passcode theme package: {}", path.display()));
        let target_ver = match self.forced_telephony_ver.as_str() {
            "TelephonyUI-10" => Some("TelephonyUI-10"),
            "TelephonyUI-9" => Some("TelephonyUI-9"),
            "TelephonyUI-8" => Some("TelephonyUI-8"),
            _ => Some("TelephonyUI-10"),
        };

        match parse_passthm_file(path, target_ver, &self.keypad_language, self.passcode_bold) {
            Ok(theme) => {
                self.keypad_textures.clear();
                for (digit, bytes) in &theme.key_previews {
                    if let Ok(img) = crate::design_library::decode_still(bytes) {
                        let rgba = img.thumbnail(256, 256).to_rgba8();
                        let color_image = egui::ColorImage::from_rgba_unmultiplied(
                            [rgba.width() as usize, rgba.height() as usize],
                            &rgba,
                        );
                        let tex = ctx.load_texture(
                            format!("keypad-{}", digit),
                            color_image,
                            egui::TextureOptions::LINEAR,
                        );
                        self.keypad_textures.push((digit.clone(), tex));
                    }
                }
                self.keypad_textures.sort_by(|a, b| a.0.cmp(&b.0));

                self.add_log(format!(
                    "Passcode theme loaded: '{}' (telephony: {}, lang: {}, bold: {}, {} assets)",
                    theme.name,
                    theme.detected_version,
                    self.keypad_language,
                    self.passcode_bold,
                    theme.items.len()
                ));
                self.status_msg = format!(
                    "{} '{}' - {} {}, {} {}, {} {}, {} {}",
                    self.language.text("Loaded"),
                    theme.name,
                    theme.items.len(),
                    self.language.text("assets"),
                    self.language.text("target:"),
                    theme.detected_version,
                    self.language.text("lang:"),
                    self.keypad_language,
                    self.language.text("bold:"),
                    self.language.text(if self.passcode_bold { "ON" } else { "OFF" })
                );
                self.theme_path = Some(path.to_path_buf());
                self.loaded_theme = Some(theme);
            }
            Err(err) => {
                self.add_log(format!("Failed to parse theme: {err:#}"));
                self.status_msg = self.language.text("No se pudo leer el tema. Revisa el archivo e inténtalo de nuevo.").to_string();
            }
        }
    }

    fn flash_theme(&mut self) {
        if !self.validate_selected_transport("Theme flash") {
            return;
        }
        let Some(udid) = self.selected_udid.clone() else {
            return;
        };
        let Some(theme) = self.loaded_theme.as_ref() else {
            self.add_log("Theme flash failed: No .passthm theme loaded.");
            self.status_msg = self
                .language
                .text("Please select a .passthm theme file first.")
                .to_string();
            return;
        };

        let items = theme.items.clone();
        self.is_busy = true;
        self.progress_step = 0;
        self.progress_total = items.len();
        self.progress_msg = self.language.text("Preparando el tema del teclado…").to_string();
        self.status_msg = self
            .language
            .text("Writing passcode theme buttons...")
            .to_string();
        let connection_mode = self.connection_mode;
        let language = self.language;
        self.add_log(format!(
            "Flashing passcode theme '{}' ({} button assets) to device {} over {}",
            theme.name,
            items.len(),
            udid,
            connection_mode.label()
        ));

        let (tx, rx) = channel();
        self.task_rx = Some(rx);

        thread::spawn(move || {
            let tx_progress = tx.clone();
            let tx_log = tx.clone();
            let res = flash_passcode_theme(
                &udid,
                connection_mode,
                &items,
                move |step, total, msg| {
                    let _ = tx_progress.send(BackgroundTaskMessage::Progress {
                        step,
                        total,
                        message: msg.to_string(),
                    });
                },
                move |msg| {
                    let _ = tx_log.send(BackgroundTaskMessage::Log(msg.to_string()));
                },
            );

            match res {
                Ok(()) => {
                    let _ = tx.send(BackgroundTaskMessage::Done(Ok(
                        language
                            .text("Passcode theme applied! Lock your iPhone to view the new keypad.")
                            .into(),
                    )));
                }
                Err(e) => {
                    let _ = tx.send(BackgroundTaskMessage::Done(Err(format!("{:#}", e))));
                }
            }
        });
    }

    fn handle_messages(&mut self) {
        let mut messages = Vec::new();
        if let Some(ref rx) = self.task_rx {
            while let Ok(msg) = rx.try_recv() {
                messages.push(msg);
            }
        }

        let mut finished = false;
        for msg in messages {
            match msg {
                BackgroundTaskMessage::Progress { step, total, message } => {
                    self.progress_step = step;
                    self.progress_total = total;
                    let localized_message = self.language.text(&message).to_string();
                    self.progress_msg = localized_message.clone();
                    let msg_str = format!("[{}/{}] {}", step, total, localized_message);
                    self.add_log(&msg_str);
                    self.status_msg = msg_str;
                }
                BackgroundTaskMessage::Log(log_line) => {
                    self.add_log(log_line);
                }
                BackgroundTaskMessage::CardFound { hash, name } => {
                    self.saved_cards = load_saved_cards();
                    self.invalidate_card_cache();
                    self.select_saved_card(&hash);
                    let display_name = self.saved_cards.iter()
                        .find(|card| card.hash == hash)
                        .map(|card| card.name.clone())
                        .unwrap_or(name);
                    self.add_log(format!("Card detected: {display_name} ({hash})"));
                    self.status_msg = format!("{}: {display_name}", self.language.text("Card detected"));
                }
                BackgroundTaskMessage::DesignSaved(path) => {
                    self.refresh_design_library();
                    // A reused older design may be outside the 100 most recent entries.
                    if !self.library_designs.iter().any(|design| design.path == path) {
                        if self.library_designs.len() >= crate::design_library::MAX_DESIGNS { self.library_designs.pop(); }
                        self.library_designs.push(crate::design_library::Design { path: path.clone(), title: "Diseño guardado automáticamente".into() });
                    }
                    self.library_selected = Some(path);
                    self.add_log("Exact prepared PNG saved and verified in the local design library before transfer.");
                }
                BackgroundTaskMessage::OriginalBackupDone { udid, hash, result } => {
                    self.is_busy = false;
                    finished = true;
                    let verified = matches!(&result,Ok(Some(_)));
                    if let Some(request) = self.auto_capture.as_mut().filter(|request| request.hash == hash && request.device.as_deref() == Some(udid.as_str())) {
                        request.finish(verified);
                    }
                    match result {
                        Ok(Some(_)) => {
                            self.add_log("Verified bank-card baseline and original state are saved on this PC.");
                            self.status_msg = self.language.text("Copia original bancaria verificada y guardada en este equipo.").to_string();
                        }
                        Ok(_) => {
                            self.add_log("Original backup capture did not produce a complete verified backup.");
                            self.status_msg = "No hay copia completa verificada. Revisa Registro y cualquier recuperación pendiente.".into();
                        }
                        Err(error) => {
                            self.add_log(format!("Original backup capture failed: {error}"));
                            self.status_msg = "No se pudo verificar la captura y devolución. Conserva las copias; revisa Registro y recuperación pendiente.".into();
                        }
                    }
                }
                BackgroundTaskMessage::Done(res) => {
                    self.is_busy = false;
                    self.scanning_syslog = false;
                    finished = true;
                    match res {
                        Ok(ok_msg) => {
                            self.add_log(format!("Operation completed: {}", ok_msg));
                            self.status_msg = ok_msg;
                        }
                        Err(err_msg) => {
                            self.add_log(format!("Operation failed: {}", err_msg));
                            self.status_msg = if err_msg.starts_with("No se guardó el diseño;") {
                                "No se pudo guardar el diseño en la biblioteca. No se inició la transferencia al iPhone.".into()
                            } else if err_msg.starts_with("No se pudo restaurar el diseño anterior") {
                                self.language.text("No se pudo restaurar el diseño anterior; el diseño nuevo no se aplicó. Conserva el iPhone conectado y revisa Registro.").to_string()
                            } else {
                                self.language.text("Operation failed. See Logs for technical details.").to_string()
                            };
                        }
                    }
                }
            }
        }
        if finished {
            self.task_rx = None;
            self.refresh_recovery_status();
        }
    }
}

pub mod md3 {
    use eframe::egui::Color32;

    // M3 Dark scheme
    pub const SURFACE: Color32 = Color32::from_rgb(7, 10, 22);
    pub const SURFACE_CONTAINER: Color32 = Color32::from_rgb(15, 20, 36);
    pub const SURFACE_CONTAINER_HIGH: Color32 = Color32::from_rgb(24, 30, 49);
    pub const SURFACE_CONTAINER_HIGHEST: Color32 = Color32::from_rgb(35, 43, 67);
    pub const ON_SURFACE: Color32 = Color32::from_rgb(231, 240, 252);
    pub const ON_SURFACE_VARIANT: Color32 = Color32::from_rgb(162, 181, 204);
    pub const OUTLINE: Color32 = Color32::from_rgb(91, 121, 155);
    pub const OUTLINE_VARIANT: Color32 = Color32::from_rgb(45, 66, 93);

    // Primary
    pub const PRIMARY: Color32 = Color32::from_rgb(108, 225, 255);
    pub const ON_PRIMARY: Color32 = Color32::from_rgb(7, 22, 34);
    pub const PRIMARY_CONTAINER: Color32 = Color32::from_rgb(25, 54, 77);
    pub const ON_PRIMARY_CONTAINER: Color32 = Color32::from_rgb(212, 240, 255);

    // Secondary
    pub const SECONDARY_CONTAINER: Color32 = Color32::from_rgb(43, 38, 73);
    pub const ON_SECONDARY_CONTAINER: Color32 = Color32::from_rgb(199, 188, 255);

    // Tertiary
    pub const TERTIARY_CONTAINER: Color32 = Color32::from_rgb(99, 59, 72);
    pub const ON_TERTIARY_CONTAINER: Color32 = Color32::from_rgb(255, 216, 228);

    // Error
    pub const ERROR: Color32 = Color32::from_rgb(242, 184, 181);
    pub const ERROR_CONTAINER: Color32 = Color32::from_rgb(140, 29, 24);

    // Extra
    pub const SUCCESS: Color32 = Color32::from_rgb(108, 232, 196);
}

fn draw_status_dot(ui: &mut egui::Ui, color: egui::Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), 4.0, color);
}

fn setup_custom_fonts(ctx: &egui::Context) {
    let windows_dir = std::env::var_os("WINDIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
    let fonts_dir = windows_dir.join("Fonts");

    let mut fonts = egui::FontDefinitions::default();

    // Primary Latin / UI font (Segoe UI is standard on Windows)
    if let Ok(segoe_bytes) = std::fs::read(fonts_dir.join("segoeui.ttf")) {
        fonts.font_data.insert(
            "segoe-ui".to_owned(),
            egui::FontData::from_owned(segoe_bytes).into(),
        );
        if let Some(prop) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
            prop.insert(0, "segoe-ui".to_owned());
        }
    }

    // Fallback CJK font for Chinese characters
    let cjk_candidates = [
        fonts_dir.join("msyh.ttc"),
        fonts_dir.join("msyh.ttf"),
        fonts_dir.join("Deng.ttf"),
        fonts_dir.join("simhei.ttf"),
        fonts_dir.join("simsun.ttc"),
        fonts_dir.join("simsunb.ttf"),
    ];

    if let Some(cjk_bytes) = cjk_candidates.iter().find_map(|p| std::fs::read(p).ok()) {
        fonts.font_data.insert(
            "windows-cjk".to_owned(),
            egui::FontData::from_owned(cjk_bytes).into(),
        );
        // Important: PUSH to the back as a fallback so it doesn't override English/Latin glyphs!
        if let Some(prop) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
            prop.push("windows-cjk".to_owned());
        }
        if let Some(mono) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
            mono.push("windows-cjk".to_owned());
        }
    }

    ctx.set_fonts(fonts);
}

fn setup_custom_theme(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();

    visuals.panel_fill = md3::SURFACE;
    visuals.window_fill = md3::SURFACE;
    visuals.extreme_bg_color = md3::SURFACE_CONTAINER;
    visuals.faint_bg_color = md3::SURFACE_CONTAINER;

    visuals.window_corner_radius = 16.into();
    visuals.menu_corner_radius = 12.into();

    visuals.widgets.noninteractive.corner_radius = 12.into();
    visuals.widgets.noninteractive.bg_fill = md3::SURFACE_CONTAINER;
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::NONE;
    visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0_f32, md3::ON_SURFACE);

    visuals.widgets.inactive.bg_fill = md3::SURFACE_CONTAINER_HIGH;
    visuals.widgets.inactive.bg_stroke = egui::Stroke::NONE;
    visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0_f32, md3::ON_SURFACE_VARIANT);
    visuals.widgets.inactive.corner_radius = 12.into();

    visuals.widgets.hovered.bg_fill = md3::SURFACE_CONTAINER_HIGHEST;
    visuals.widgets.hovered.bg_stroke = egui::Stroke::NONE;
    visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0_f32, md3::ON_SURFACE);
    visuals.widgets.hovered.corner_radius = 12.into();

    visuals.widgets.active.bg_fill = md3::PRIMARY_CONTAINER;
    visuals.widgets.active.bg_stroke = egui::Stroke::NONE;
    visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0_f32, md3::ON_PRIMARY_CONTAINER);
    visuals.widgets.active.corner_radius = 12.into();

    visuals.widgets.open.bg_fill = md3::SURFACE_CONTAINER_HIGHEST;
    visuals.widgets.open.corner_radius = 12.into();
    visuals.widgets.open.bg_stroke = egui::Stroke::NONE;

    visuals.selection.bg_fill = md3::PRIMARY_CONTAINER;
    visuals.selection.stroke = egui::Stroke::new(1.0_f32, md3::PRIMARY);

    ctx.set_visuals(visuals);

    ctx.style_mut(|style| {
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(16.0, 8.0);
    });
}

fn m3_card<R>(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::new()
        .fill(md3::SURFACE_CONTAINER)
        .stroke(egui::Stroke::new(1.0_f32, md3::OUTLINE_VARIANT))
        .corner_radius(14)
        .inner_margin(egui::Margin::same(16))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.vertical(add_contents).inner
        })
        .inner
}

fn m3_button_filled(ui: &mut egui::Ui, label: &str) -> bool {
    let btn = egui::Button::new(
        egui::RichText::new(label).size(13.0).color(md3::ON_PRIMARY),
    )
    .fill(md3::PRIMARY)
    .corner_radius(10)
    .stroke(egui::Stroke::NONE);
    ui.add(btn.min_size(egui::vec2(112.0, 40.0))).clicked()
}

fn m3_button_tonal(ui: &mut egui::Ui, label: &str) -> bool {
    let btn = egui::Button::new(
        egui::RichText::new(label).size(13.0).color(md3::ON_SECONDARY_CONTAINER),
    )
    .fill(md3::SECONDARY_CONTAINER)
    .corner_radius(10)
    .stroke(egui::Stroke::NONE);
    ui.add(btn.min_size(egui::vec2(112.0, 40.0))).clicked()
}

fn m3_button_outlined(ui: &mut egui::Ui, label: &str) -> bool {
    let btn = egui::Button::new(
        egui::RichText::new(label).size(12.5).color(md3::PRIMARY),
    )
    .fill(egui::Color32::TRANSPARENT)
    .corner_radius(10)
    .stroke(egui::Stroke::new(1.0_f32, md3::OUTLINE));
    ui.add(btn.min_size(egui::vec2(112.0, 40.0))).clicked()
}

fn studio_button(ui: &mut egui::Ui, label: &str, enabled: bool, primary: bool) -> bool {
    ui.add_enabled(enabled, egui::Button::new(egui::RichText::new(label).strong().size(12.0)
        .color(if primary { md3::ON_PRIMARY } else { md3::ON_SECONDARY_CONTAINER }))
        .fill(if primary { md3::PRIMARY } else { md3::SECONDARY_CONTAINER })
        .corner_radius(10).min_size(egui::vec2(ui.available_width(), 40.0))).clicked()
}

fn studio_artwork(ui: &mut egui::Ui, texture: Option<&egui::TextureHandle>, width: f32) {
    let size = egui::vec2(width, width * 969.0 / 1536.0);
    if let Some(texture) = texture {
        ui.add(egui::Image::new((texture.id(), texture.size_vec2())).fit_to_exact_size(size).maintain_aspect_ratio(true).corner_radius(12));
    } else {
        let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
        ui.painter().rect(rect, 12.0, md3::SURFACE_CONTAINER_HIGH, egui::Stroke::new(1.0_f32, md3::OUTLINE_VARIANT), egui::StrokeKind::Inside);
        ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "Original pendiente de guardar", egui::FontId::proportional(13.0), md3::ON_SURFACE_VARIANT);
    }
}

fn m3_tab(ui: &mut egui::Ui, current: &mut AppTab, target: AppTab, label: &str) {
    let selected = *current == target;
    let (bg, fg) = if selected {
        (md3::SECONDARY_CONTAINER, md3::ON_SECONDARY_CONTAINER)
    } else {
        (egui::Color32::TRANSPARENT, md3::ON_SURFACE_VARIANT)
    };
    let btn = egui::Button::new(
        egui::RichText::new(label).size(12.5).color(fg),
    )
    .fill(bg)
    .corner_radius(10)
    .stroke(egui::Stroke::NONE);
    if ui.add(btn).clicked() {
        *current = target;
    }
}


impl eframe::App for AirCardApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_messages();
        self.tick_card_cache(ctx);
        self.tick_auto_capture();
        let language = self.language;

        if self.is_busy || self.scanning_syslog {
            ctx.request_repaint_after(std::time::Duration::from_millis(60));
        }

        // Top bar
        egui::TopBottomPanel::top("header")
            .frame(
                egui::Frame::new()
                    .fill(md3::SURFACE)
                    .inner_margin(egui::Margin::symmetric(20, 10)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.add(egui::Image::new((self.brand_texture.id(), egui::vec2(28.0, 28.0))));
                    ui.label(
                        egui::RichText::new("Wallet Studio")
                            .strong()
                            .size(18.0)
                            .color(md3::ON_SURFACE),
                    );
                    ui.label(
                        egui::RichText::new(format!("v{}", crate::update_info::VERSION))
                            .size(11.0)
                            .color(md3::ON_SURFACE_VARIANT),
                    );

                    ui.add_space(20.0);
                    m3_tab(ui, &mut self.current_tab, AppTab::Wallet, language.text("Wallet"));
                    m3_tab(ui, &mut self.current_tab, AppTab::Passcode, language.text("Passcode"));
                    m3_tab(ui, &mut self.current_tab, AppTab::Help, language.text("Help"));
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("DISPOSITIVO / CONEXIÓN").strong().size(10.0).color(md3::ON_SURFACE_VARIANT));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if m3_button_outlined(ui, language.text("Refresh")) {
                            self.refresh_devices();
                        }

                        ui.add_space(6.0);
                        let controls_enabled = !self.is_busy && !self.scanning_syslog;
                        let mut next_mode = self.connection_mode;
                        ui.add_enabled_ui(controls_enabled, |ui| {
                            egui::ComboBox::from_id_salt("connection_mode_combo")
                                .selected_text(language.text(next_mode.label()))
                                .width(135.0)
                                .show_ui(ui, |ui| {
                                    for mode in ConnectionMode::ALL {
                                        ui.selectable_value(
                                            &mut next_mode,
                                            mode,
                                            language.text(mode.label()),
                                        );
                                    }
                                });
                        });
                        if next_mode != self.connection_mode {
                            self.connection_mode = next_mode;
                            self.add_log(format!(
                                "Transport mode changed to {}.",
                                self.connection_mode.label()
                            ));
                            self.status_msg = format!(
                                "{}: {}",
                                language.text("Transport mode"),
                                language.text(self.connection_mode.label())
                            );
                        }

                        ui.add_space(6.0);
                        let mut next_udid = self.selected_udid.clone();
                        let selected_label = self
                            .devices
                            .iter()
                            .find(|device| Some(&device.udid) == self.selected_udid.as_ref())
                            .map(|device| format!("{} [{}]", device.name, device.transport_summary()))
                            .unwrap_or_else(|| language.text("No device").to_string());

                        ui.add_enabled_ui(controls_enabled && !self.devices.is_empty(), |ui| {
                            egui::ComboBox::from_id_salt("device_selector_combo")
                                .selected_text(selected_label)
                                .width(175.0)
                                .show_ui(ui, |ui| {
                                    for device in &self.devices {
                                        ui.selectable_value(
                                            &mut next_udid,
                                            Some(device.udid.clone()),
                                            format!("{} [{}]", device.name, device.transport_summary()),
                                        );
                                    }
                                });
                        });
                        if next_udid != self.selected_udid {
                            self.selected_udid = next_udid;
                            self.invalidate_card_cache();
                            if let Some(selected) = self.selected_udid.clone() {
                                self.add_log(format!("Selected device: {}", selected));
                            }
                        }

                        ui.add_space(8.0);
                        let connection_ready = self.selected_transport_available();
                        draw_status_dot(
                            ui,
                            if connection_ready { md3::SUCCESS } else { md3::ERROR },
                        );
                        ui.label(
                            egui::RichText::new(if connection_ready {
                                language.text("Ready")
                            } else {
                                language.text("Unavailable")
                            })
                            .size(12.0)
                            .color(if connection_ready {
                                md3::ON_SURFACE
                            } else {
                                md3::ON_SURFACE_VARIANT
                            }),
                        )
                        .on_hover_text(&self.apple_status);
                    });
                });
            });

        self.tick_gif(ctx);

        // Status bar
        egui::TopBottomPanel::bottom("status_bar")
            .frame(
                egui::Frame::new()
                    .fill(md3::SURFACE)
                    .inner_margin(egui::Margin::symmetric(20, 8)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let dot_col = if self.is_busy || self.scanning_syslog {
                        md3::PRIMARY
                    } else if self.status_msg.starts_with("Error") || self.status_msg.starts_with("Failed") {
                        md3::ERROR
                    } else {
                        md3::SUCCESS
                    };
                    draw_status_dot(ui, dot_col);
                    if self.is_busy || self.scanning_syslog { ui.spinner(); }
                    ui.label(egui::RichText::new(&self.status_msg).size(11.5).color(md3::ON_SURFACE_VARIANT));

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let btn_text = if self.show_logs_window {
                            language.text("Logs [x]")
                        } else {
                            language.text("Logs")
                        };
                        let btn = egui::Button::new(
                            egui::RichText::new(btn_text).size(11.0).color(
                                if self.show_logs_window { md3::ON_PRIMARY_CONTAINER } else { md3::ON_SURFACE_VARIANT }
                            ),
                        )
                        .fill(if self.show_logs_window { md3::PRIMARY_CONTAINER } else { egui::Color32::TRANSPARENT })
                        .corner_radius(10)
                        .stroke(egui::Stroke::new(1.0_f32, if self.show_logs_window { md3::PRIMARY } else { md3::OUTLINE_VARIANT }));
                        if ui.add(btn).clicked() {
                            self.show_logs_window = !self.show_logs_window;
                        }

                        ui.add_space(8.0);

                        let mut next_language = self.language;
                        egui::ComboBox::from_id_salt("language_combo_bottom")
                            .selected_text(language.option_label(next_language))
                            .width(105.0)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut next_language,
                                    Language::Spanish,
                                    language.option_label(Language::Spanish),
                                );
                                ui.selectable_value(
                                    &mut next_language,
                                    Language::English,
                                    language.option_label(Language::English),
                                );
                                ui.selectable_value(
                                    &mut next_language,
                                    Language::SimplifiedChinese,
                                    language.option_label(Language::SimplifiedChinese),
                                );
                            });
                        if next_language != self.language {
                            self.language = next_language;
                            self.language.save();
                            self.status_msg = self.language.text("Language changed.").to_string();
                        }
                    });
                });
            });

        // Central - same SURFACE fill as header/status for flat look
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(md3::SURFACE)
                    .inner_margin(egui::Margin::same(16)),
            )
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    match self.current_tab {
                        AppTab::Wallet => self.show_wallet_tab(ctx, ui),
                        AppTab::Passcode => self.show_passcode_tab(ctx, ui),
                        AppTab::Help => self.show_help_tab(ui),
                    }
                });
            });

        let mut show_logs = self.show_logs_window;
        let mut file_saved_msg: Option<String> = None;
        if show_logs {
            egui::Window::new(language.text("Logs"))
                .open(&mut show_logs)
                .default_size([540.0, 300.0])
                .min_size([360.0, 180.0])
                .show(ctx, |ui| {
                    ui.label(egui::RichText::new("El registro interno puede contener datos privados. Copiar y guardar ocultan los identificadores y nombres conocidos; revisa cualquier dato restante antes de compartir.").size(11.0).color(md3::ON_SURFACE_VARIANT));
                    ui.horizontal(|ui| {
                        if m3_button_tonal(ui, "Copiar registro protegido") {
                            ctx.copy_text(self.redacted_logs());
                        }
                        if m3_button_outlined(ui, "Guardar registro protegido…") {
                            if let Some(path) = rfd::FileDialog::new()
                                .set_file_name("wallet-studio-diagnostico.log")
                                .add_filter("Log files", &["log", "txt"])
                                .save_file()
                            {
                                let content = self.redacted_logs().replace('\n', "\r\n");
                                file_saved_msg = Some(match std::fs::write(&path, content) {
                                    Ok(()) => format!("Registro guardado: {}", path.display()),
                                    Err(_) => "No se pudo guardar el registro. Revisa la ubicación y los permisos.".into(),
                                });
                            }
                        }
                        if m3_button_outlined(ui, language.text("Clear")) {
                            self.logs.clear();
                        }
                        ui.label(
                            egui::RichText::new(format!(
                                "{} {}",
                                self.logs.len(),
                                language.text("entries")
                            ))
                                .size(11.0)
                                .color(md3::ON_SURFACE_VARIANT),
                        );
                    });
                    ui.add_space(8.0);
                    egui::Frame::new()
                        .fill(md3::SURFACE_CONTAINER_HIGH)
                        .corner_radius(12)
                        .inner_margin(egui::Margin::same(10))
                        .show(ui, |ui| {
                            egui::ScrollArea::vertical()
                                .stick_to_bottom(true)
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    if self.logs.is_empty() {
                                        ui.label(egui::RichText::new(language.text("No events logged yet.")).size(11.0).color(md3::ON_SURFACE_VARIANT));
                                    } else {
                                        for line in &self.logs {
                                            ui.label(
                                                egui::RichText::new(line)
                                                    .size(10.5)
                                                    .monospace()
                                                    .color(md3::ON_SURFACE),
                                            );
                                        }
                                    }
                                });
                        });
                });
            self.show_logs_window = show_logs;
            if let Some(msg) = file_saved_msg {
                self.add_log(msg);
            }
        }
        self.show_crop_editor(ctx);
    }
}

impl AirCardApp {
    fn select_saved_card(&mut self, hash: &str) {
        self.card_hash = hash.to_string();
        self.gallery_open = false;
        self.card_name_draft = self.saved_cards.iter()
            .find(|card| card.hash == hash)
            .map(|card| card.name.clone())
            .unwrap_or_default();
        self.rename_error = None;
        let connected = self.selected_udid.as_deref().filter(|selected|
            self.devices.iter().any(|device| device.udid == *selected));
        let expected = capture_device_for_selection(connected,self.offline_baseline_device.get(hash).map(String::as_str));
        self.auto_capture = Some(AutoCapture::selected(hash.to_string(),expected));
    }

    fn save_current_card_name(&mut self) {
        match crate::scanner::rename_saved_card(self.card_hash.trim(), &self.card_name_draft) {
            Ok(()) => {
                self.saved_cards = load_saved_cards();
                self.card_name_draft = self.saved_cards.iter()
                    .find(|card| card.hash == self.card_hash.trim())
                    .map(|card| card.name.clone())
                    .unwrap_or_default();
                self.rename_error = None;
                self.status_msg = self.language.text("Nombre guardado").to_string();
                self.invalidate_card_cache();
            }
            Err(error) => {
                self.rename_error = Some(self.language.text("No se pudo guardar el nombre. Revisa el texto y vuelve a intentarlo.").to_string());
                self.add_log(format!("Card name was not saved: {error:#}"));
            }
        }
    }

    fn invalidate_card_cache(&mut self) {
        self.card_snapshots.invalidate();
        self.card_cache_generation.store(self.card_snapshots.epoch(),Ordering::Relaxed);
        self.card_cache_refresh = true;
        self.original_thumbnails.clear();
        self.applied_thumbnails.clear();
        self.offline_baseline_device.clear();
        self.recovery_status_error = Some("Comprobando el estado local de recuperación…".into());
    }

    fn snapshot(&self, hash: &str) -> Option<&CardSnapshot> {
        self.card_snapshots.get(hash).filter(|snapshot|
            self.selected_udid.is_none() || snapshot.udid.as_ref() == self.selected_udid.as_ref())
    }
    fn original_verified(&self, hash: &str) -> bool {
        self.snapshot(hash).is_some_and(|snapshot| snapshot.original == OriginalState::Verified)
    }
    fn original_thumbnail(&mut self, _ctx: &egui::Context, hash: &str) -> Option<egui::TextureHandle> {
        self.original_thumbnails.get(hash).cloned()
    }
    fn applied_thumbnail(&mut self, _ctx: &egui::Context, hash: &str) -> Option<egui::TextureHandle> {
        self.applied_thumbnails.get(hash).cloned()
    }
    fn bank_intent_phase(&self, _udid: &str, hash: &str) -> Result<Option<String>, ()> {
        self.snapshot(hash).map(|snapshot| snapshot.phase.clone()).unwrap_or(Err(()))
    }
    fn selected_usb_ready(&self) -> bool {
        self.apple_ready && self.connection_mode != ConnectionMode::Wifi
            && self.selected_udid.as_ref().is_some_and(|udid| self.devices.iter().any(|device|
                device.udid == *udid && device.has_transport(DeviceTransport::Usb)
                    && crate::device::wallet_version_warning(&device.ios_version).is_none()))
    }

    fn tick_card_cache(&mut self, ctx: &egui::Context) {
        let mut messages = Vec::new();
        if let Some(rx) = &self.card_cache_rx {
            for _ in 0..4 { match rx.try_recv() { Ok(message) => messages.push(message), Err(_) => break } }
        }
        for message in messages {
            match message {
                CardCacheMessage::Card { epoch, hash, mut snapshot } => {
                    if epoch != self.card_snapshots.epoch() { continue; }
                    if let Some(known) = &snapshot.known_device { self.offline_baseline_device.insert(hash.clone(),known.clone()); }
                    for (image, textures, label) in [(&mut snapshot.original_image,&mut self.original_thumbnails,"original"),
                        (&mut snapshot.applied_image,&mut self.applied_thumbnails,"applied")] {
                        if let Some(image) = image.take() {
                            textures.insert(hash.clone(),ctx.load_texture(format!("{label}-{epoch}-{hash}"),image,egui::TextureOptions::LINEAR));
                        }
                    }
                    self.card_snapshots.insert(epoch,hash,snapshot);
                }
                CardCacheMessage::Finished { epoch, recovery } => {
                    self.card_cache_rx = None;
                    if epoch != self.card_snapshots.epoch() { continue; }
                    match recovery {
                        Ok(pending) => { self.pending_recoveries = pending; self.recovery_status_error = None; }
                        Err(error) => self.recovery_status_error = Some(error),
                    }
                }
            }
        }
        if self.card_cache_rx.is_none() && self.card_cache_refresh && !self.is_busy && !self.scanning_syslog {
            self.card_cache_refresh = false;
            let epoch = self.card_snapshots.epoch();
            let generation = Arc::clone(&self.card_cache_generation);
            let selected = self.selected_udid.clone();
            let mut hashes: Vec<_> = self.saved_cards.iter().map(|card| card.hash.clone()).collect();
            let selected_hash = self.card_hash.trim().to_string();
            if crate::wallet_export::is_exact_card_hash(&selected_hash) && !hashes.contains(&selected_hash) { hashes.push(selected_hash); }
            let (tx,rx) = channel(); self.card_cache_rx = Some(rx);
            let ctx = ctx.clone();
            thread::spawn(move || {
                for hash in hashes {
                    if generation.load(Ordering::Relaxed) != epoch { break; }
                    let snapshot = load_card_snapshot(selected.as_deref(),&hash);
                    if tx.send(CardCacheMessage::Card { epoch,hash,snapshot }).is_err() { return; }
                    ctx.request_repaint();
                }
                let recovery = crate::wallet_export::pending_recovery_operations().map_err(|error|format!("{error:#}"));
                let _ = tx.send(CardCacheMessage::Finished { epoch,recovery }); ctx.request_repaint();
            });
        }
        if self.card_cache_rx.is_some() { ctx.request_repaint_after(std::time::Duration::from_millis(80)); }
    }

    fn tick_auto_capture(&mut self) {
        let Some(request) = self.auto_capture.as_ref() else { return; };
        if request.hash != self.card_hash { return; }
        let Some(snapshot) = self.snapshot(&request.hash) else { return; };
        let original = snapshot.original;
        let known = snapshot.known_device.clone();
        let safe = !self.is_busy && !self.scanning_syslog && self.selected_usb_ready()
            && self.pending_recoveries.is_empty() && self.recovery_status_error.is_none()
            && self.card_cache_rx.is_none() && snapshot.phase.is_ok();
        if self.auto_capture.as_mut().is_some_and(|request|
            request.should_start(self.selected_udid.as_deref(),known.as_deref(),original,safe)) {
            self.capture_original_backup();
        }
    }
    fn show_wallet_tab(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        let language = self.language;
        let version_warning = self.devices.iter()
            .find(|device| Some(&device.udid) == self.selected_udid.as_ref())
            .and_then(|device| crate::device::wallet_version_warning(&device.ios_version));
        {
            ui.add_space(8.0);
            m3_card(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new(language.text("Tu espacio Wallet")).strong().size(24.0).color(md3::ON_SURFACE));
                        ui.label(egui::RichText::new(language.text("DISEÑA  /  PERSONALIZA  /  RESTAURA")).size(12.0).color(md3::ON_SURFACE_VARIANT));
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let ready = self.selected_transport_available();
                        let color = if ready { md3::SUCCESS } else { md3::ERROR };
                        draw_status_dot(ui, color);
                        let device = self.devices.iter().find(|d| Some(&d.udid) == self.selected_udid.as_ref());
                        let title = if let Some(device) = device {
                            format!("{} · {} · iOS {}", language.text(if ready { "iPhone conectado" } else { "iPhone no disponible" }), device.product_type, device.ios_version)
                        } else {
                            language.text("Conecta un iPhone desbloqueado").to_string()
                        };
                        ui.label(egui::RichText::new(title).size(11.5).color(if ready { md3::SUCCESS } else { md3::ON_SURFACE_VARIANT }));
                    });
                });
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("01 / BIBLIOTECA").size(11.0).strong().color(md3::PRIMARY));
                    ui.add_space(12.0);
                    if self.scanning_syslog {
                        ui.spinner();
                        ui.label(egui::RichText::new(language.text("Buscando tarjeta… abre Wallet y tócala en el iPhone.")).size(11.0).color(md3::ON_SURFACE_VARIANT));
                        if m3_button_outlined(ui, language.text("Detener")) { self.toggle_syslog_scan(); }
                    } else {
                        let add_card = egui::Button::new(egui::RichText::new(language.text("Añadir tarjeta")).strong().color(md3::ON_PRIMARY)).fill(md3::PRIMARY).corner_radius(10).min_size(egui::vec2(140.0, 40.0));
                        if ui.add_enabled(!self.is_busy, add_card).clicked() { self.toggle_syslog_scan(); }
                    }
                });
            });

            if let Some(warning) = &version_warning {
                ui.add_space(8.0);
                m3_card(ui, |ui| {
                    ui.label(egui::RichText::new(warning).color(md3::ERROR));
                    ui.label("Puedes preparar diseños y editar nombres. La conexión USB por sí sola no verifica compatibilidad con Wallet.");
                });
            }

            ui.add_space(10.0);
            let gallery = egui::CollapsingHeader::new(format!("{} · {}", language.text("Tus tarjetas guardadas"), self.saved_cards.len()))
                .id_salt("wallet-gallery-panel").open(Some(self.gallery_open)).show(ui, |ui| {
            if self.saved_cards.is_empty() {
                m3_card(ui, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.label(egui::RichText::new(language.text("Aún no hay tarjetas guardadas")).strong().size(14.0).color(md3::ON_SURFACE));
                        ui.label(egui::RichText::new(language.text("Pulsa «Añadir tarjeta», abre Wallet en el iPhone y toca la tarjeta que quieras personalizar.")).size(11.5).color(md3::ON_SURFACE_VARIANT));
                    });
                });
            } else {
                egui::ScrollArea::horizontal().id_salt("saved-card-gallery").auto_shrink([false, true]).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        for card in self.saved_cards.clone() {
                            let original_verified = self.original_verified(&card.hash);
                            let current_design = self.applied_thumbnail(ctx, &card.hash);
                            let original_thumbnail = self.original_thumbnail(ctx, &card.hash);
                            let selected = self.card_hash == card.hash;
                            if card_gallery::draw_card_tile(ui, &card.name, &card.hash, current_design.as_ref(), original_thumbnail.as_ref(), original_verified, selected, language) && !self.is_busy {
                                self.select_saved_card(&card.hash);
                                ctx.request_repaint();
                            }
                        }
                    });
                });
            }

            });
            if gallery.header_response.clicked() { self.gallery_open = !self.gallery_open; }
            ui.add_space(10.0);
            ui.columns(2, |workspace| {
            let ui = &mut workspace[0];
            let selected_card = self.saved_cards.iter().find(|card| card.hash == self.card_hash).cloned();
            if let Some(card) = selected_card {
                m3_card(ui, |ui| {
                    ui.set_min_height(430.0);
                    ui.label(egui::RichText::new("02 / TARJETA SELECCIONADA").size(11.0).strong().color(md3::ON_SECONDARY_CONTAINER));
                    ui.label(egui::RichText::new(&card.name).size(19.0).strong().color(md3::ON_SURFACE));
                    let current = self.applied_thumbnail(ctx, &card.hash);
                    let original = self.original_thumbnail(ctx, &card.hash);
                    ui.add_space(10.0);
                    studio_artwork(ui, current.as_ref().or(original.as_ref()), (ui.available_width()-8.0).min(460.0));
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if let Some(texture) = original.as_ref() {
                            ui.add(egui::Image::new((texture.id(), texture.size_vec2())).fit_to_exact_size(egui::vec2(72.0, 46.0)).maintain_aspect_ratio(true).corner_radius(5));
                        }
                        ui.vertical(|ui| {
                            ui.label(egui::RichText::new(if current.is_some() { "Último diseño aplicado y verificado" } else { "Original guardado" }).size(11.0).strong().color(md3::PRIMARY));
                            ui.label(egui::RichText::new("Copia local · no es una lectura actual del iPhone").size(10.0).color(md3::ON_SURFACE_VARIANT));
                        });
                    });
                    ui.add_space(10.0);

                    ui.vertical(|ui| {
                    ui.label(egui::RichText::new(language.text("Nombre de la tarjeta")).strong().size(13.0).color(md3::ON_SURFACE));
                        ui.add_enabled_ui(!self.is_busy, |ui| {
                            ui.add(egui::TextEdit::singleline(&mut self.card_name_draft).hint_text(language.text("Por ejemplo, Transporte" )).desired_width(140.0));
                            if m3_button_tonal(ui, language.text("Guardar nombre")) { self.save_current_card_name(); }
                        });
                    });
                    if let Some(error) = &self.rename_error {
                        ui.label(egui::RichText::new(error).size(11.0).color(md3::ERROR));
                    }
                    ui.add_space(4.0);
                    let verified = self.original_verified(&card.hash);
                    let copy_status = if verified && self.selected_udid.is_none() {
                        "Copia original verificada en este equipo. Conecta el iPhone para usarla."
                    } else if verified {
                        "Copia original verificada para este iPhone"
                    } else if self.offline_baseline_device.contains_key(&card.hash) {
                        "Hay una copia para otro iPhone. Conecta el dispositivo correcto."
                    } else {
                        "Falta una copia original verificable"
                    };
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new(language.text(copy_status)).size(11.5).color(if verified { md3::SUCCESS } else { md3::ERROR }));
                        if verified {
                            ui.label(egui::RichText::new(language.text("La miniatura procede de la copia local; no representa una lectura actual del iPhone.")).size(10.5).color(md3::ON_SURFACE_VARIANT));
                        }
                    });
                    egui::CollapsingHeader::new(language.text("Detalles técnicos")).default_open(false).show(ui, |ui| {
                        ui.label(egui::RichText::new(language.text("Identificador de tarjeta")).size(10.5).color(md3::ON_SURFACE_VARIANT));
                        if ui.add_enabled(!self.is_busy, egui::TextEdit::singleline(&mut self.card_hash).desired_width(f32::INFINITY)).changed() {
                            self.auto_capture = None; self.invalidate_card_cache();
                        }
                        if let Some(udid) = &self.selected_udid {
                            ui.label(egui::RichText::new(format!("{}: {}", language.text("Dispositivo"), udid)).size(10.0).color(md3::ON_SURFACE_VARIANT));
                        }
                    });
                });
            } else {
                m3_card(ui, |ui| {
                    ui.set_min_height(430.0);
                    ui.label(egui::RichText::new("02 / TARJETA SELECCIONADA").size(11.0).strong().color(md3::ON_SECONDARY_CONTAINER));
                    ui.label(egui::RichText::new("Tu tarjeta, a tu manera").size(19.0).strong().color(md3::ON_SURFACE));
                    ui.add_space(10.0);
                    studio_artwork(ui, None, (ui.available_width()-8.0).min(460.0));
                    ui.add_space(10.0);
                    ui.label(egui::RichText::new(language.text("Selecciona una tarjeta guardada para continuar.")).size(12.0).color(md3::ON_SURFACE_VARIANT));
                    egui::CollapsingHeader::new(language.text("Detalles técnicos / introducir identificador")).default_open(false).show(ui, |ui| {
                        ui.label(egui::RichText::new(language.text("Identificador de tarjeta")).size(10.5).color(md3::ON_SURFACE_VARIANT));
                        if ui.add_enabled(!self.is_busy, egui::TextEdit::singleline(&mut self.card_hash).hint_text(language.text("Identificador técnico de la tarjeta" )).desired_width(f32::INFINITY)).changed() {
                            self.auto_capture = None; self.invalidate_card_cache();
                        }
                    });
                });
            }

            let ui = &mut workspace[1];
            m3_card(ui, |ui| {
                ui.set_min_height(430.0);
                ui.label(egui::RichText::new("03 / NUEVO DISEÑO").size(11.0).strong().color(md3::PRIMARY));
                ui.label(egui::RichText::new("Prepara tu próxima tarjeta").size(19.0).strong().color(md3::ON_SURFACE));
                ui.add_space(8.0);
                ui.label(egui::RichText::new("Biblioteca de diseños").strong().size(12.0).color(md3::ON_SURFACE));
                ui.add_enabled_ui(!self.is_busy, |ui| {
                    let title = self.library_designs.iter().find(|design| Some(&design.path) == self.library_selected.as_ref())
                        .map(|design| design.title.as_str()).unwrap_or("Sin diseños en la carpeta");
                    egui::ComboBox::from_id_salt("design-library").width(ui.available_width() - 24.0)
                        .selected_text(title).show_ui(ui, |ui| {
                            for design in &self.library_designs {
                                ui.selectable_value(&mut self.library_selected, Some(design.path.clone()), &design.title);
                            }
                        });
                    ui.columns(2, |buttons| {
                        if studio_button(&mut buttons[0], "Usar este diseño", self.library_selected.is_some(), true) {
                            if let Some(path) = self.library_selected.clone() { self.load_skin_path(ctx, path); }
                        }
                        if studio_button(&mut buttons[1], "Actualizar biblioteca", true, false) { self.refresh_design_library(); }
                    });
                });
                if let Some(error) = &self.library_error {
                    ui.label(egui::RichText::new(error).size(10.5).color(md3::ERROR));
                } else {
                    ui.label(egui::RichText::new(format!("{} diseños recientes · guardado automático en designs", self.library_designs.len())).size(10.5).color(md3::ON_SURFACE_VARIANT));
                }
                ui.add_space(6.0);
                ui.columns(2, |buttons| {
                    if studio_button(&mut buttons[0], "Elegir imagen…", !self.is_busy, true) { self.select_skin(ctx); }
                    if studio_button(&mut buttons[1], "Exportar PNG", !self.is_busy && self.gif_playback.is_none() && self.crop_draft.is_none() && !self.crop_dirty && self.skin.is_some(), false) { self.save_prepared_png(); }
                });
                ui.add_space(8.0);
                ui.label(egui::RichText::new("PNG · JPG · WebP · GIF / salida estática PNG + PDF").size(10.5).color(md3::ON_SURFACE_VARIANT));
                if ui.add_enabled(!self.is_busy && self.gif_playback.is_none() && self.source_image.is_some() && self.crop_draft.is_none(), egui::Button::new("Recortar imagen…")).clicked() {
                    let crop = self.current_crop();
                    self.crop_draft = Some(if crop.edges.is_some() { crop } else { CardCrop::full_image() });
                }
                if self.source_image.is_some() && self.current_crop() != CardCrop::default()
                    && ui.add_enabled(!self.is_busy && self.gif_playback.is_none() && self.crop_draft.is_none(), egui::Button::new("Restablecer recorte")).clicked() {
                    self.crop_focus = [0.5, 0.5];
                    self.crop_zoom = 1.0;
                    self.crop_edges = None;
                    self.rebuild_skin_from_source();
                }
                if !self.gif_frames.is_empty() {
                    ui.horizontal(|ui| {
                        let playing = self.gif_playback.is_some();
                        if ui.add_enabled(!self.is_busy && self.crop_draft.is_none(), egui::Button::new(if playing { "Pausar y elegir fotograma" } else { "Reproducir GIF" })).clicked() {
                            if playing { self.freeze_gif(ctx, self.gif_frame); }
                            else { self.gif_playback = Some((Instant::now(), self.gif_frame)); ctx.request_repaint(); }
                        }
                        if ui.add_enabled(!self.is_busy && self.crop_draft.is_none(), egui::Button::new("Reiniciar")).clicked() {
                            self.gif_playback = Some((Instant::now(), 0));
                            self.gif_frame = 0;
                            if let Some(image) = crate::animated_input::selected_frame(&self.gif_frames, 0) { self.update_gif_texture(ctx, &image); }
                            ctx.request_repaint();
                        }
                    });
                    let mut selected = self.gif_frame;
                    if ui.add_enabled(!self.is_busy && self.gif_playback.is_none() && self.crop_draft.is_none(), egui::Slider::new(&mut selected, 0..=self.gif_frames.len()-1).text("Fotograma")).changed() {
                        self.freeze_gif(ctx, selected);
                    }
                    ui.label(egui::RichText::new("Animación solo en Wallet Studio. Pausa para exportar o aplicar un fotograma estático en Apple Wallet.").size(10.5).color(md3::ON_SECONDARY_CONTAINER));
                }
                    if let Some(skin) = &self.skin {
                        let filename = self.source_path.as_ref().and_then(|path| path.file_name()).and_then(|name| name.to_str()).unwrap_or("Diseño");
                        ui.label(egui::RichText::new(format!("{} · {:.0} KB", filename, skin.png.len() as f32 / 1024.0)).size(10.5).color(md3::PRIMARY));
                    } else {
                        ui.label(egui::RichText::new(language.text("No hay un diseño seleccionado.")).size(11.0).color(md3::ON_SURFACE_VARIANT));
                    }
                    ui.add_space(8.0);
                    let width = (ui.available_width() - 8.0).clamp(160.0, 460.0);
                    let height = width * (969.0 / 1536.0);
                    if let Some(texture) = &self.source_texture {
                        let uv = self.source_image.as_ref().map(|source| {
                            self.current_crop().uv(source.width(), source.height())
                        }).unwrap_or([0.0, 0.0, 1.0, 1.0]);
                        let crop_rect = egui::Rect::from_min_max(egui::pos2(uv[0], uv[1]), egui::pos2(uv[2], uv[3]));
                        let response = ui.add(egui::Image::new((texture.id(), texture.size_vec2())).uv(crop_rect).fit_to_exact_size(egui::vec2(width, height)).maintain_aspect_ratio(false).sense(if self.is_busy || self.gif_playback.is_some() || self.crop_draft.is_some() { egui::Sense::hover() } else { egui::Sense::drag() }));
                        if response.dragged() && !self.is_busy && self.gif_playback.is_none() && self.crop_draft.is_none() {
                            let mut crop = self.current_crop();
                            let delta = response.drag_motion();
                            if crop.edges.is_some() {
                                crop.translate(self.source_image.as_ref().unwrap().width(), self.source_image.as_ref().unwrap().height(),
                                    [-delta.x / width * 0.2, -delta.y / height * 0.2]);
                            } else {
                                crop.focus[0] = (crop.focus[0] - delta.x / width * 0.2).clamp(0.0, 1.0);
                                crop.focus[1] = (crop.focus[1] - delta.y / height * 0.2).clamp(0.0, 1.0);
                            }
                            self.crop_focus = crop.focus;
                            self.crop_edges = crop.edges;
                            self.crop_dirty = true;
                            ctx.request_repaint();
                        }
                        if response.drag_stopped() && self.crop_dirty { self.rebuild_skin_from_source(); }
                        ui.label(egui::RichText::new(language.text("Arrastra la imagen para ajustar el encuadre.")).size(10.0).color(md3::ON_SURFACE_VARIANT));
                    } else {
                        let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
                        ui.painter().rect_filled(rect, 14.0, md3::SURFACE_CONTAINER_HIGH);
                        ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, language.text("Imagen no disponible"), egui::FontId::proportional(12.0), md3::ON_SURFACE_VARIANT);
                    }
                });
            });

            ui.add_space(10.0);
            let selected_hash = self.card_hash.trim().to_string();
            let original_verified = self.original_verified(&selected_hash);
            let selected_intent_phase = self.selected_udid.clone().map_or(Err(()), |udid|
                self.bank_intent_phase(&udid, &selected_hash));
            m3_card(ui, |ui| {
                ui.label(egui::RichText::new("04 / TRANSFERENCIA SEGURA").size(11.0).strong().color(md3::PRIMARY));
                ui.add_space(8.0);
                ui.columns(2, |actions| {
                    let ui = &mut actions[0];
                    let connected = self.selected_transport_available() && version_warning.is_none()
                        && self.pending_recoveries.is_empty() && self.recovery_status_error.is_none();
                    let apply_plan = cached_bank_apply_plan(&selected_intent_phase);
                    let can_apply = connected && !self.is_busy && self.gif_playback.is_none() && self.crop_draft.is_none() && !self.crop_dirty && !self.scanning_syslog && !selected_hash.is_empty() && self.skin.is_some()
                && self.selected_udid.as_ref().is_some_and(|_| {
                    crate::flasher::BANK_APPLY_READY
                        && original_verified
                                && apply_plan != crate::flasher::BankApplyPlan::Block
                });
                    let apply_label = if apply_plan == crate::flasher::BankApplyPlan::RestoreThenApply {
                        language.text("Sustituir diseño")
                    } else { language.text("Aplicar diseño") };
                    let apply_button = egui::Button::new(egui::RichText::new(apply_label).strong().color(if can_apply { md3::ON_PRIMARY } else { md3::ON_SURFACE_VARIANT }))
                        .fill(if can_apply { md3::PRIMARY } else { md3::SURFACE_CONTAINER_HIGH }).corner_radius(14).min_size(egui::vec2(ui.available_width(), 44.0));
                    let ui = &mut actions[0];
                    if ui.add_enabled(can_apply, apply_button).clicked() { self.flash_card(); }

            let can_restore = connected && !self.is_busy && !self.scanning_syslog && !selected_hash.is_empty()
                && self.snapshot(&selected_hash).is_some_and(|snapshot| snapshot.restore);
                    let restore_button = egui::Button::new(egui::RichText::new(language.text("Restaurar copia original")).strong().color(if can_restore { md3::ON_SECONDARY_CONTAINER } else { md3::ON_SURFACE_VARIANT }))
                        .fill(if can_restore { md3::SECONDARY_CONTAINER } else { md3::SURFACE_CONTAINER_HIGH }).corner_radius(14).min_size(egui::vec2(ui.available_width(), 44.0));
                    let ui = &mut actions[1];
                    if ui.add_enabled(can_restore, restore_button).clicked() { self.restore_original_card(); }
                });
                if !crate::flasher::BANK_APPLY_READY {
                    ui.label(egui::RichText::new("Aplicar está bloqueado: la caché de Wallet aún no tiene retirada recuperable y comprobación exacta. No se escribirá ningún diseño.").size(11.0).color(md3::ERROR));
                }
                if !original_verified {
                    let message = match self.auto_capture.as_ref().map(|capture|capture.phase) {
                        Some(CapturePhase::Running) => "Guardando automáticamente el original y verificando su devolución…",
                        Some(CapturePhase::Failed) => "La captura no terminó. Conserva las copias y resuelve cualquier recuperación antes de reintentar.",
                        Some(CapturePhase::Blocked) => "Hay copia o historial incompleto. La captura automática está bloqueada para conservar el original y la evidencia.",
                        Some(CapturePhase::ForeignOriginal) => "Hay una copia original para otro iPhone, pero falta una verificable para el seleccionado. No se capturará su diseño actual como original; conserva las copias y comprueba su procedencia.",
                        Some(CapturePhase::Waiting) if self.auto_capture.as_ref().is_some_and(|capture|capture.device.is_none()) => "Conecta el iPhone correcto por USB y vuelve a seleccionar esta tarjeta para guardar el original.",
                        Some(CapturePhase::Waiting) => "La copia se guardará automáticamente cuando el iPhone seleccionado esté disponible por USB y no haya recuperaciones pendientes.",
                        _ => "Selecciona una tarjeta: su original se guardará automáticamente antes de aplicar diseños.",
                    };
                    ui.label(egui::RichText::new(message).size(11.0).color(md3::ON_SURFACE_VARIANT));
                    ui.label(egui::RichText::new("La captura puede mover originales temporalmente y comprueba su devolución. Cierra Wallet y Libros; mantén el iPhone desbloqueado y el USB conectado.").size(10.5).color(md3::ON_SURFACE_VARIANT));
                    if self.auto_capture.as_ref().is_some_and(|capture|capture.phase == CapturePhase::Failed)
                        && ui.add_enabled(!self.is_busy && self.selected_usb_ready() && self.pending_recoveries.is_empty()
                            && self.recovery_status_error.is_none(),egui::Button::new("Reintentar guardar original")).clicked() {
                        if let Some(capture) = &mut self.auto_capture { capture.retry(); }
                        self.invalidate_card_cache();
                    }
                }
                if self.recovery_status_error.is_some() {
                    let checking = self.card_cache_refresh || self.card_cache_rx.is_some();
                    ui.label(egui::RichText::new(if checking { "Comprobando copias y recuperaciones en segundo plano…" }
                        else { "No se pudo comprobar el registro de recuperación. Las transferencias están bloqueadas; conserva las copias." })
                        .color(if checking { md3::ON_SURFACE_VARIANT } else { md3::ERROR }));
                }
                if !self.pending_recoveries.is_empty() {
                    ui.label(egui::RichText::new("Hay una recuperación pendiente. Conserva las copias y conecta el mismo iPhone.").color(md3::ERROR));
                    let pending = self.pending_recoveries.iter().find(|entry|
                        self.selected_udid.as_ref().is_some_and(|udid| udid.eq_ignore_ascii_case(&entry.udid))
                        && entry.original_path.is_some()).cloned();
                    if let Some(pending) = pending {
                        ui.label(format!("Archivo pendiente: {}", pending.leaf));
                        if ui.add_enabled(!self.is_busy && !self.scanning_syslog
                            && self.selected_transport_available() && version_warning.is_none(),
                            egui::Button::new("Recuperar original pendiente")).clicked() {
                            self.recover_pending_original(pending.operation_dir);
                        }
                    }
                }
                if ui.add_enabled(!self.is_busy, egui::Button::new("Comprobar recuperaciones").small()).clicked() {
                    self.refresh_recovery_status();
                    if let Some(error) = self.recovery_status_error.clone() { self.add_log(error); }
                }
                if self.is_busy {
                    if self.progress_total > 0 { ui.add(egui::ProgressBar::new(self.progress_step as f32 / self.progress_total as f32).animate(true)); }
                    ui.label(egui::RichText::new(&self.progress_msg).size(10.5).color(md3::PRIMARY));
                }
                ui.label(egui::RichText::new(&self.status_msg).size(11.0).color(md3::ON_SURFACE_VARIANT));
            });
        }
    }
    fn show_passcode_tab(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        let language = self.language;
        ui.columns(2, |cols| {
            // Left: config
            let left = &mut cols[0];
            m3_card(left, |ui| {
                ui.label(egui::RichText::new(language.text("Passcode Theme")).strong().size(16.0).color(md3::ON_SURFACE));
                ui.add_space(4.0);
                ui.label(egui::RichText::new(language.text("Custom lockscreen keypad from Cowabunga or Nugget")).size(12.0).color(md3::ON_SURFACE_VARIANT));
                ui.add_space(16.0);

                // Theme file
                ui.label(egui::RichText::new(language.text("Theme Package")).strong().size(12.0).color(md3::ON_SURFACE));
                ui.label(egui::RichText::new(language.text("Choose a .passthm archive containing dialer artwork")).size(11.0).color(md3::ON_SURFACE_VARIANT));
                ui.add_space(4.0);
                if m3_button_filled(ui, language.text("Choose .passthm...")) { self.select_theme_file(ctx); }

                if let Some(theme) = &self.loaded_theme {
                    let fname = self.theme_path.as_ref()
                        .and_then(|p| p.file_name()).and_then(|n| n.to_str()).unwrap_or("theme.passthm");
                    ui.add_space(4.0);
                    ui.label(egui::RichText::new(format!("{} - {} assets", fname, theme.items.len())).size(11.0).color(md3::PRIMARY));
                }

                ui.add_space(16.0);

                // iOS version
                ui.label(egui::RichText::new(language.text("Target iOS Cache")).strong().size(12.0).color(md3::ON_SURFACE));
                ui.label(egui::RichText::new(language.text("Select cache format based on connected iOS version")).size(11.0).color(md3::ON_SURFACE_VARIANT));
                ui.add_space(4.0);
                let combo_w = (ui.available_width() - 4.0).max(150.0);
                let mut ver_changed = false;
                egui::ComboBox::from_id_salt("telephony_combo")
                    .width(combo_w)
                    .selected_text(language.text(&self.forced_telephony_ver))
                    .show_ui(ui, |ui| {
                        ver_changed |= ui.selectable_value(&mut self.forced_telephony_ver, "Auto (TelephonyUI-10)".into(), language.text("Auto (TelephonyUI-10)")).clicked();
                        ver_changed |= ui.selectable_value(&mut self.forced_telephony_ver, "TelephonyUI-10".into(), language.text("TelephonyUI-10 (iOS 18+)")).clicked();
                        ver_changed |= ui.selectable_value(&mut self.forced_telephony_ver, "TelephonyUI-9".into(), language.text("TelephonyUI-9 (iOS 16-17)")).clicked();
                        ver_changed |= ui.selectable_value(&mut self.forced_telephony_ver, "TelephonyUI-8".into(), language.text("TelephonyUI-8 (Legacy)")).clicked();
                    });

                if ver_changed {
                    if let Some(path) = self.theme_path.clone() {
                        self.load_theme_from_path(ctx, &path);
                    }
                }

                ui.add_space(16.0);

                // Keypad Language
                ui.label(egui::RichText::new(language.text("Keypad Language")).strong().size(12.0).color(md3::ON_SURFACE));
                ui.label(egui::RichText::new(language.text("Subtext alphabet layout (English, Russian, Ukrainian, Japanese, or Universal)")).size(11.0).color(md3::ON_SURFACE_VARIANT));
                ui.add_space(4.0);
                let mut lang_changed = false;
                egui::ComboBox::from_id_salt("keypad_lang_combo")
                    .width(combo_w)
                    .selected_text(egui::RichText::new(language.text(&self.keypad_language)).color(md3::ON_SURFACE))
                    .show_ui(ui, |ui| {
                        lang_changed |= ui.selectable_value(&mut self.keypad_language, "English".into(), language.text("English")).clicked();
                        lang_changed |= ui.selectable_value(&mut self.keypad_language, "Russian".into(), language.text("Russian")).clicked();
                        lang_changed |= ui.selectable_value(&mut self.keypad_language, "Ukrainian".into(), language.text("Ukrainian")).clicked();
                        lang_changed |= ui.selectable_value(&mut self.keypad_language, "Japanese".into(), language.text("Japanese")).clicked();
                        lang_changed |= ui.selectable_value(&mut self.keypad_language, "All Languages (Universal)".into(), language.text("All Languages (Universal)")).clicked();
                    });

                if lang_changed {
                    if let Some(path) = self.theme_path.clone() {
                        self.load_theme_from_path(ctx, &path);
                    }
                }

                ui.add_space(10.0);

                // Bold Font Toggle
                let mut bold_changed = false;
                ui.horizontal(|ui| {
                    if ui.checkbox(&mut self.passcode_bold, egui::RichText::new(language.text("Bold Text (iOS Accessibility)")).strong().size(12.0).color(md3::ON_SURFACE)).changed() {
                        bold_changed = true;
                    }
                });
                ui.label(
                    egui::RichText::new(language.text("Generates *-bold.png for devices with Bold Text turned ON in iPhone Settings -> Display"))
                        .size(11.0)
                        .color(md3::ON_SURFACE_VARIANT),
                );

                if bold_changed {
                    if let Some(path) = self.theme_path.clone() {
                        self.load_theme_from_path(ctx, &path);
                    }
                }

                ui.add_space(16.0);

                // Apply
                ui.label(egui::RichText::new(language.text("Write to iPhone")).strong().size(12.0).color(md3::ON_SURFACE));
                ui.add_space(4.0);

                let can_flash = !self.is_busy
                    && self.selected_transport_available()
                    && self.loaded_theme.is_some();
                let flash_btn = egui::Button::new(
                    egui::RichText::new(language.text("Apply Passcode Theme")).strong().size(14.0)
                        .color(if can_flash { md3::ON_PRIMARY } else { md3::ON_SURFACE_VARIANT }),
                )
                .fill(if can_flash { md3::PRIMARY } else { md3::SURFACE_CONTAINER_HIGH })
                .corner_radius(10).stroke(egui::Stroke::NONE)
                .min_size(egui::vec2(ui.available_width(), 40.0));

                let resp = ui.add_enabled(can_flash, flash_btn);
                if resp.clicked() { self.flash_theme(); }
                if !can_flash {
                    let mut r = Vec::new();
                    if self.selected_udid.is_none() { r.push(language.text("connect iPhone")); }
                    else if !self.selected_transport_available() { r.push(language.text("choose available transport")); }
                    if self.loaded_theme.is_none() { r.push(language.text("select theme")); }
                    if !r.is_empty() { resp.on_disabled_hover_text(format!("{}{}", language.text("Need: "), r.join(", "))); }
                }

                if self.is_busy {
                    ui.add_space(8.0);
                    if self.progress_total > 0 {
                        ui.add(egui::ProgressBar::new(self.progress_step as f32 / self.progress_total as f32).animate(true));
                    }
                    ui.label(egui::RichText::new(&self.progress_msg).size(11.0).color(md3::PRIMARY));
                }
            });

            // Right: preview
            let right = &mut cols[1];
            m3_card(right, |ui| {
                ui.label(egui::RichText::new(language.text("Keypad Preview")).strong().size(16.0).color(md3::ON_SURFACE));
                ui.add_space(4.0);
                ui.label(egui::RichText::new(language.text("Dialer button artwork")).size(12.0).color(md3::ON_SURFACE_VARIANT));
                ui.add_space(12.0);

                let pass_w = (ui.available_width() - 8.0).clamp(240.0, 360.0);
                let pass_h = 265.0;

                ui.vertical_centered(|ui| {
                    if self.keypad_textures.is_empty() {
                        let (rect, _) = ui.allocate_exact_size(egui::vec2(pass_w, pass_h), egui::Sense::hover());
                        let painter = ui.painter();
                        painter.rect_filled(rect, 16.0, md3::SURFACE_CONTAINER_HIGH);
                        painter.text(rect.center(), egui::Align2::CENTER_CENTER,
                            language.text("No theme loaded"), egui::FontId::proportional(14.0), md3::ON_SURFACE_VARIANT);
                    } else {
                        let (rect, _) = ui.allocate_exact_size(egui::vec2(pass_w, pass_h), egui::Sense::hover());
                        let painter = ui.painter();
                        painter.rect_filled(rect, 16.0, md3::SURFACE_CONTAINER_HIGH);
                        ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
                            ui.vertical_centered(|ui| {
                                ui.add_space(10.0);
                                const DIALER_LAYOUT: &[&[&str]] = &[
                                    &["1", "2", "3"],
                                    &["4", "5", "6"],
                                    &["7", "8", "9"],
                                    &["", "0", ""],
                                ];
                                egui::Grid::new("keypad_grid")
                                    .spacing([18.0, 6.0])
                                    .show(ui, |ui| {
                                        for row in DIALER_LAYOUT {
                                            for &d in *row {
                                                if d.is_empty() {
                                                    ui.allocate_exact_size(egui::vec2(44.0, 50.0), egui::Sense::hover());
                                                } else if let Some((_, tex)) = self.keypad_textures.iter().find(|(k, _)| k == d) {
                                                    ui.vertical_centered(|ui| {
                                                        egui::Frame::new()
                                                            .fill(md3::SURFACE)
                                                            .corner_radius(12)
                                                            .inner_margin(3)
                                                            .show(ui, |ui| { ui.image((tex.id(), egui::vec2(40.0, 40.0))); });
                                                        ui.label(egui::RichText::new(d).size(9.5).color(md3::ON_SURFACE_VARIANT));
                                                    });
                                                } else {
                                                    ui.vertical_centered(|ui| {
                                                        egui::Frame::new()
                                                            .fill(md3::SURFACE)
                                                            .corner_radius(12)
                                                            .inner_margin(3)
                                                            .show(ui, |ui| {
                                                                let (btn_rect, _) = ui.allocate_exact_size(egui::vec2(40.0, 40.0), egui::Sense::hover());
                                                                ui.painter().rect_filled(btn_rect, 8.0, md3::SURFACE_CONTAINER);
                                                                ui.painter().text(btn_rect.center(), egui::Align2::CENTER_CENTER, d, egui::FontId::proportional(14.0), md3::ON_SURFACE_VARIANT);
                                                            });
                                                        ui.label(egui::RichText::new(d).size(9.5).color(md3::ON_SURFACE_VARIANT));
                                                    });
                                                }
                                            }
                                            ui.end_row();
                                        }
                                    });
                            });
                        });
                    }
                });

                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(language.text("3x4 Keypad")).size(11.0).color(md3::ON_SURFACE_VARIANT));
                    ui.label(egui::RichText::new("|").size(11.0).color(md3::OUTLINE_VARIANT));
                    ui.label(egui::RichText::new("TelephonyUI").size(11.0).color(md3::ON_SURFACE_VARIANT));
                    ui.label(egui::RichText::new("|").size(11.0).color(md3::OUTLINE_VARIANT));
                    if self.loaded_theme.is_some() {
                        ui.label(egui::RichText::new(language.text("Ready")).size(11.0).color(md3::SUCCESS));
                    } else {
                        ui.label(egui::RichText::new(language.text("No theme")).size(11.0).color(md3::ON_SURFACE_VARIANT));
                    }
                });
                ui.add_space(8.0);
                ui.label(egui::RichText::new(language.text("After applying, lock your iPhone to see the new keypad.")).size(11.0).color(md3::ON_SURFACE_VARIANT));
            });
        });
    }

    fn show_help_tab(&mut self, ui: &mut egui::Ui) {
        let language = self.language;
        m3_card(ui, |ui| {
            ui.label(egui::RichText::new("Actualizaciones").strong().size(16.0).color(md3::ON_SURFACE));
            ui.label(format!("Versión instalada: {}", crate::update_info::VERSION));
            ui.label(if crate::update_info::PUBLIC_CHANNEL_PUBLISHED {
                "Consulta la versión publicada y descarga la actualización en el navegador."
            } else {
                "El canal público de descargas todavía no está publicado. No hay búsqueda automática de versiones."
            });
            if ui.add_enabled(crate::update_info::PUBLIC_CHANNEL_PUBLISHED,
                egui::Button::new("Buscar actualizaciones")).clicked() {
                ui.ctx().open_url(egui::OpenUrl::new_tab(crate::update_info::PUBLIC_DOWNLOAD_URL));
            }
            ui.label(egui::RichText::new("Actualización manual: cierra la aplicación y sustituye los archivos del programa. Conserva designs y las copias originales locales; no borres los datos de Wallet Studio.").size(11.0).color(md3::ON_SURFACE_VARIANT));
        });
        ui.add_space(12.0);
        ui.columns(2, |cols| {
            let left = &mut cols[0];
            m3_card(left, |ui| {
                ui.label(egui::RichText::new(language.text("Setup & Card Hash Guide")).strong().size(16.0).color(md3::ON_SURFACE));
                ui.add_space(4.0);
                ui.label(egui::RichText::new(language.text("Everything you need to connect and capture your card")).size(12.0).color(md3::ON_SURFACE_VARIANT));
                ui.add_space(16.0);

                ui.label(egui::RichText::new(language.text("Prerequisites")).strong().size(12.0).color(md3::ON_SURFACE));
                ui.add_space(6.0);
                ui.label(egui::RichText::new(language.text("- 64-bit iTunes or Apple Mobile Device Support installed")).size(11.5).color(md3::ON_SURFACE_VARIANT));
                ui.label(egui::RichText::new(language.text("- First-time setup: connect by USB and tap \"Trust this Computer\"")).size(11.5).color(md3::ON_SURFACE_VARIANT));
                ui.label(egui::RichText::new(language.text("- WiFi: enable WiFi sync, then use the same local network")).size(11.5).color(md3::ON_SURFACE_VARIANT));
                ui.label(egui::RichText::new(language.text("- Select Auto, USB only, or WiFi only in the top bar")).size(11.5).color(md3::ON_SURFACE_VARIANT));

                ui.add_space(18.0);

                ui.label(egui::RichText::new(language.text("Finding Your Card Hash")).strong().size(12.0).color(md3::ON_SURFACE));
                ui.add_space(6.0);
                ui.label(egui::RichText::new(language.text("1. Pulsa «Añadir tarjeta» en Tarjetas.")).size(11.5).color(md3::ON_SURFACE_VARIANT));
                ui.label(egui::RichText::new(language.text("2. Abre Wallet y toca la tarjeta en el iPhone.")).size(11.5).color(md3::ON_SURFACE_VARIANT));
                ui.label(egui::RichText::new(language.text("3. La actividad reciente puede detectar y guardar la tarjeta.")).size(11.5).color(md3::ON_SURFACE_VARIANT));
                ui.label(egui::RichText::new(language.text("4. Puedes cambiar su nombre; el identificador permanece igual.")).size(11.5).color(md3::ON_SURFACE_VARIANT));
            });

            let right = &mut cols[1];
            m3_card(right, |ui| {
                ui.label(egui::RichText::new(language.text("Activation & Theme Guide")).strong().size(16.0).color(md3::ON_SURFACE));
                ui.add_space(4.0);
                ui.label(egui::RichText::new("Selecciona la tarjeta; el original se guarda automáticamente y se verifica antes de aplicar diseños.").size(12.0).color(md3::ON_SURFACE_VARIANT));
                ui.add_space(16.0);

                ui.label(egui::RichText::new(language.text("Activating Apple Wallet Skin")).strong().size(12.0).color(md3::ON_SURFACE));
                ui.add_space(6.0);
                ui.label(egui::RichText::new(language.text("1. Selecciona una tarjeta con su iPhone conectado por USB; el original se guarda automáticamente.")).size(11.5).color(md3::ON_SURFACE_VARIANT));
                ui.label(egui::RichText::new("2. Cierra Wallet y Libros; mantén el iPhone desbloqueado y el USB conectado. Espera «Original guardado».").size(11.5).color(md3::ON_SURFACE_VARIANT));
                ui.label(egui::RichText::new("3. La captura puede mover originales temporalmente y comprueba su devolución. Si falla, conserva las copias, resuelve «Recuperar original pendiente» y usa el reintento explícito.").size(11.5).color(md3::ON_SURFACE_VARIANT));
                ui.label(egui::RichText::new(language.text("4. Tras una aplicación confirmada, cierra y vuelve a abrir Wallet para comprobar el resultado.")).size(11.5).color(md3::ON_SURFACE_VARIANT));

                ui.add_space(18.0);

                ui.label(egui::RichText::new(language.text("Passcode Themes (.passthm)")).strong().size(12.0).color(md3::ON_SURFACE));
                ui.add_space(6.0);
                ui.label(egui::RichText::new(language.text("- Compatible with Cowabunga & Nugget theme packages")).size(11.5).color(md3::ON_SURFACE_VARIANT));
                ui.label(egui::RichText::new(language.text("- iOS 18+: Select \"Auto (TelephonyUI-10)\"")).size(11.5).color(md3::ON_SURFACE_VARIANT));
                ui.label(egui::RichText::new(language.text("- iOS 16-17: Select \"TelephonyUI-9\"")).size(11.5).color(md3::ON_SURFACE_VARIANT));
                ui.label(egui::RichText::new(language.text("- Lock screen to verify your updated keypad artwork")).size(11.5).color(md3::ON_SURFACE_VARIANT));
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_export_redacts_literal_overlapping_identifiers_without_recursive_replacement() {
        let raw = "ABC-123 aBc Tarjeta (1) C:\\Users\\Dani\\backup.png; dato oculto";
        let redacted = redact_log_text(raw, vec!["ABC".into(), "ABC-123".into(),
            "Tarjeta (1)".into(), "C:\\Users\\Dani".into(), "dato oculto".into(), String::new()]);
        assert_eq!(redacted, "[dato oculto] [dato oculto] [dato oculto] [dato oculto]\\backup.png; [dato oculto]");
        assert_eq!(redact_log_text(raw, Vec::new()), raw);
    }

    #[test]
    fn verified_replacement_and_recovery_ui_states_are_distinct() {
        use crate::flasher::BankApplyPlan;
        assert_eq!(cached_bank_apply_plan(&Ok(Some("applied".into()))), BankApplyPlan::RestoreThenApply);
        assert_eq!(cached_bank_apply_plan(&Ok(Some("restored".into()))), BankApplyPlan::Apply);
        assert_eq!(cached_bank_apply_plan(&Ok(Some("prepared".into()))), BankApplyPlan::Block);
        assert_eq!(cached_bank_apply_plan(&Err(())), BankApplyPlan::Block);
    }

    #[test]
    fn square_and_tall_preview_uv_match_the_card_canvas() {
        let expected_ratio = 1536.0_f32 / 969.0_f32;
        for (width, height) in [(1200, 1200), (600, 1200)] {
            let uv = crate::image_skin::crop_uv_for_card(width, height, 0.5, 0.5);
            assert!(uv[0] >= 0.0 && uv[1] >= 0.0 && uv[2] <= 1.0 && uv[3] <= 1.0);
            let visible_ratio = ((uv[2] - uv[0]) * width as f32)
                / ((uv[3] - uv[1]) * height as f32);
            assert!((visible_ratio - expected_ratio).abs() < 0.01);
        }
    }

    #[test]
    fn test_setup_custom_fonts() {
        let ctx = egui::Context::default();
        setup_custom_fonts(&ctx);
        // egui compiles font textures during run()
        let _ = ctx.run(Default::default(), |ctx| {
            ctx.fonts_mut(|fonts| {
                let w1 = fonts.glyph_width(&egui::FontId::proportional(14.0), 'A');
                let w2 = fonts.glyph_width(&egui::FontId::proportional(14.0), '中');
                assert!(w1 > 0.0);
                assert!(w2 > 0.0);
            });
        });
    }
}


