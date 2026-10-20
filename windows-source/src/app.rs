use std::collections::HashMap;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::thread;
use std::time::Instant;

use anyhow::Context;
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

// Retire the old receiver before starting a replacement worker. A scanner may
// enqueue CardFound and Done together, or Done may arrive just after CardFound.
fn retire_task_receiver<T>(receiver: &mut Option<Receiver<T>>, finished: bool, card_found: bool) -> bool {
    if finished || card_found {
        *receiver = None;
        true
    } else {
        false
    }
}

struct CardSnapshot {
    udid: Option<String>, known_device: Option<String>, original: OriginalState,
    reference_device:Option<String>,
    restore: bool, phase: Result<Option<String>, ()>,
    original_image: Option<egui::ColorImage>, applied_image: Option<egui::ColorImage>,
}
fn unique_reference_device_at(root:&Path,hash:&str)->Option<String> {
    crate::storage::safe_chain(root).ok()?;
    let mut devices=std::collections::BTreeSet::new();
    for entry in std::fs::read_dir(root).ok()? {
        let path=entry.ok()?.path();let (udid,card_hash)=crate::bank_backup::bundle_identity(&path).ok()?;
        if card_hash==hash {devices.insert(udid);}
    }
    if devices.len()==1 {devices.into_iter().next()} else {None}
}
fn attach_reference(snapshot:&mut CardSnapshot,device:String,reference:CardSnapshot) {
    // Presentation only. Exact selected-device proof, restoration and phases
    // must never be replaced by another phone's saved artwork.
    if reference.original_image.is_some() || reference.applied_image.is_some() {
        snapshot.original_image=reference.original_image;snapshot.applied_image=reference.applied_image;snapshot.reference_device=Some(device);
    }
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
    let mut result = CardSnapshot { udid: udid.clone(), known_device,reference_device:None,
        original: OriginalState::Missing, restore: false, phase: Ok(None),
        original_image: None, applied_image: None };
    let Some(udid) = udid else { return result; };
    let card_review_blocked = crate::storage::ensure_card_ready_at(&crate::storage::root(),&udid,hash).is_err();
    let bank = crate::bank_backup::load_baseline(&udid,hash);
    let has_bank_baseline = matches!(&bank, Ok(Some(_)));
    let mut bank_verified = false;
    match bank {
        Ok(Some(baseline)) => {
            if card_review_blocked {
                result.original = OriginalState::Blocked;
            } else {
                bank_verified = true;
                result.original = OriginalState::Verified;
            }
            result.original_image = thumbnail(&baseline.remote_artwork_bytes);
        }
        Err(_) => { result.original = OriginalState::Blocked; if card_review_blocked { result.phase=Err(());return result; } }
        Ok(None) => {
            if card_review_blocked { result.original=OriginalState::Blocked;result.phase=Err(());return result; }
            if crate::wallet_backup::backup_evidence_exists(&udid,hash) {
                match crate::wallet_backup::load_original_assets(&udid,hash) {
                    Ok(assets) => { result.original = OriginalState::Verified;
                        result.original_image = assets.iter().filter(|(leaf,_)| leaf.ends_with(".png"))
                            .find_map(|(_,bytes)| thumbnail(bytes)); }
                    Err(_) => result.original = OriginalState::Blocked,
                }
            }
        }
    }
    match crate::bank_changes::load_latest_intent(&udid,hash) {
        Ok(intent) => {
            result.phase = Ok(intent.as_ref().map(|intent| intent.phase.clone()));
            if intent.is_some() && result.original == OriginalState::Missing { result.original = OriginalState::Blocked; }
            result.restore = if card_review_blocked {
                has_bank_baseline && intent.as_ref().is_some_and(|intent| intent.phase != "restored")
            } else {
                result.original == OriginalState::Verified && (!bank_verified
                    || intent.as_ref().is_some_and(|intent| intent.phase != "restored"))
            };
            if let Some(intent) = intent.filter(|intent| intent.phase == "applied") {
                result.applied_image = intent.assets.get("cardBackgroundCombined@3x.png").and_then(|bytes| thumbnail(bytes));
                if result.original == OriginalState::Missing { result.original = OriginalState::Blocked; }
            }
        }
        Err(_) => { result.phase = Err(()); if result.original == OriginalState::Missing { result.original = OriginalState::Blocked; } }
    }
    if selected.is_some() && result.original_image.is_none() && result.applied_image.is_none() {
        if let Some(reference)=unique_reference_device_at(&crate::storage::root().join("bank-backups"),hash).filter(|device|Some(device.as_str())!=selected) {
            let artwork=load_card_snapshot(Some(&reference),hash);attach_reference(&mut result,reference,artwork);
        }
    }
    result
}

#[derive(PartialEq, Eq)]
enum AppTab {
    Wallet,
    Library,
    Passcode,
    Help,
    Account,
}

fn cloud_sync_status(session:&crate::cloud_sync::Session,previously_authenticated:bool,tx:&std::sync::mpsc::Sender<CloudEvent>,ctx:&egui::Context)->(bool,bool,bool,bool,String) {
    match crate::cloud_sync::account_app_license(session) {
        Ok(false)=>return (false,true,false,false,"Tu cuenta conserva derechos históricos de diseños. Necesitas una licencia vigente de Wallet Studio para utilizar el iPhone y sus copias.".into()),
        Err(error)=>{let retained=crate::cloud_sync::retained_session_after_failure(&error);return (false,retained,false,retained,crate::cloud_sync::session_connection_failure_message(&error));},
        Ok(true)=>{},
    }
    match crate::cloud_sync::synchronize_report_progress(session,|status| {let _=tx.send(CloudEvent::Progress(status));ctx.request_repaint();}) {
        Ok(report)=>(true,true,report.complete(),false,report.message()),
        Err(error)=>{let authenticated=crate::cloud_sync::authenticated_after_sync_failure(&error,previously_authenticated);let retained=crate::cloud_sync::retained_session_after_failure(&error);(authenticated,retained,false,!authenticated && retained,crate::cloud_sync::sync_failure_message(&error))},
    }
}
struct CloudMessage { authenticated:bool,library_authenticated:bool,session_pending:bool,storage_ready:bool,configured:bool,synced:bool,server:String,status:String,session:Option<crate::cloud_sync::Session> }
enum CloudEvent {Progress(&'static str),Restored(crate::cloud_sync::Session),Complete(CloudMessage)}

/// Decoding and PNG preparation run before switching away from the library.
struct LoadedLibraryDesign {
    path: PathBuf,
    source_image: DynamicImage,
    frames: Vec<crate::animated_input::GifFrame>,
    skin: PreparedSkin,
    preview: egui::ColorImage,
}
fn prepare_library_design(path: PathBuf) -> anyhow::Result<LoadedLibraryDesign> {
    let bytes = crate::design_library::read_image(&path)?;
    let (source_image, frames) = if path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("gif")) {
        let frames = crate::animated_input::decode_gif(Cursor::new(bytes))?;
        let image = frames.first().context("El GIF no tiene fotogramas")?.image.clone();
        (image, frames)
    } else { (crate::design_library::decode_still(&bytes)?, Vec::new()) };
    let skin = PreparedSkin::from_image_with_focus(source_image.clone(), 0.5, 0.5)?;
    let rgba = source_image.thumbnail(source_image.width().min(2048), source_image.height().min(2048)).to_rgba8();
    let preview = egui::ColorImage::from_rgba_unmultiplied([rgba.width() as usize, rgba.height() as usize], rgba.as_raw());
    Ok(LoadedLibraryDesign { path, source_image, frames, skin, preview })
}
fn paint_library_preview(ui:&egui::Ui,rect:egui::Rect,path:&Path,texture:Option<&egui::TextureHandle>,failed:bool) {
    ui.painter().rect_filled(rect,8.0,md3::SURFACE_CONTAINER_HIGH);
    let opacity=ui.ctx().animate_bool_with_time(egui::Id::new(("library-loaded",path)),texture.is_some(),0.18);
    if let Some(texture)=texture {
        let size=texture.size_vec2();let scale=(rect.width()/size.x).min(rect.height()/size.y);
        egui::Image::new((texture.id(),size)).tint(egui::Color32::WHITE.linear_multiply(opacity))
            .paint_at(ui,egui::Rect::from_center_size(rect.center(),size*scale));
    } else {
        ui.painter().text(rect.center(),egui::Align2::CENTER_CENTER,if failed {"Imagen no disponible"} else {"Cargando…"},egui::FontId::proportional(12.0),md3::ON_SURFACE_VARIANT);
    }
}

enum BackgroundTaskMessage {
    Progress { step: usize, total: usize, message: String },
    Log(String),
    CardFound { udid: String, hash: String, name: String },
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
    cloud_rx:Option<Receiver<CloudEvent>>,
    activation_rx:Option<Receiver<bool>>,account_stage:u8,account_code:String,account_confirmation:String,account_authenticated:bool,library_authenticated:bool,session_pending:bool,
    account_devices:Vec<crate::cloud_sync::RegisteredComputer>,account_devices_rx:Option<Receiver<Result<Vec<crate::cloud_sync::RegisteredComputer>,()>>>,
    account_recovery:bool,account_reset_devices:bool,
    auth_preview:bool,
    activation_requested:Option<Instant>,password_visible:bool,
    cloud_session:Option<crate::cloud_sync::Session>,cloud_sync_queued:bool,last_cloud_sync:Instant,
    storage_ready:bool,cloud_configured:bool,cloud_synced:bool,
    cloud_status:String,account_server:String,account_email:String,account_password:String,account_link_consent:bool,
    update_rx:Option<Receiver<String>>,update_status:String,
    brand:crate::brand::BrandLogo,
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
    library_selection: crate::library_loader::Selection,
    library_error: Option<String>,
    library_textures: HashMap<PathBuf,egui::TextureHandle>,
    purchased_designs: Vec<(crate::design_library::PurchasedDesign,PathBuf)>,
    purchased_status: String,
    purchased_rx: Option<Receiver<(String,Result<Vec<(crate::design_library::PurchasedDesign,PathBuf)>,String>)>>,
    purchased_use_rx: Option<Receiver<(String,u64,Result<LoadedLibraryDesign,String>)>>,
    library_owner: String,
    library_previews: crate::library_loader::Previews,
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
    fn seed_demo(&mut self,ctx:&egui::Context) {
        self.storage_ready=true; self.card_cache_refresh=false; self.recovery_status_error=None;
        self.cloud_status="Demostración aislada · cuenta y teléfono desactivados".into();self.update_status="Demostración · sin conexiones de red".into();
        self.status_msg="Demostración · imágenes sintéticas · ninguna transferencia al iPhone".into();
        let empty_name=std::env::current_exe().ok().and_then(|path|path.file_stem().map(|name|name.to_string_lossy().contains(" Demo Empty")||name.to_string_lossy().eq_ignore_ascii_case("WSDEMOEM"))).unwrap_or(false);
        if empty_name || std::env::var("WALLET_STUDIO_DEMO_EMPTY").as_deref()==Ok("1") { self.saved_cards.clear();return; }
        let image=DynamicImage::ImageRgba8(image::RgbaImage::from_fn(640,404,|x,y| {
            let accent=(x/120+y/85)%3==0;image::Rgba(if accent {[43+(x/12) as u8,125+(y/8) as u8,153,255]} else {[23+(x/35) as u8,35+(y/24) as u8,53,255]})
        }));
        let rgba=image.to_rgba8();let texture=ctx.load_texture("synthetic-demo-art",egui::ColorImage::from_rgba_unmultiplied([640,404],rgba.as_raw()),egui::TextureOptions::LINEAR);
        self.saved_cards=[("AAAAAAAAAAAAAAAAAAAAAAAAAAA=","Tarjeta de ejemplo"),("AQEBAQEBAQEBAQEBAQEBAQEBAQE=","Viajes y transporte"),("AgICAgICAgICAgICAgICAgICAgI=","Compras del día a día")].into_iter().map(|(hash,name)|SavedCard {hash:hash.into(),name:name.into()}).collect();
        for card in &self.saved_cards {
            self.original_thumbnails.insert(card.hash.clone(),texture.clone());
            self.card_snapshots.insert(self.card_snapshots.epoch(),card.hash.clone(),CardSnapshot {udid:None,known_device:None,reference_device:None,original:OriginalState::Verified,restore:false,phase:Ok(None),original_image:None,applied_image:None});
        }
        self.card_hash=self.saved_cards[0].hash.clone();self.card_name_draft=self.saved_cards[0].name.clone();
        self.source_texture=Some(texture);self.source_image=Some(image);self.rebuild_skin_from_source();
        self.library_owner="demo".into();
        if let (Some(skin),Ok(root))=(&self.skin,crate::design_library::library_dir()) {let _=crate::design_library::save_prepared_png(&root,&skin.png);}
        self.refresh_design_library();
        self.purchased_status="DEMOSTRACIÓN AISLADA · miniaturas locales de ejemplo; sin compras ni conexiones de red.".into();
        if let Some(source)=std::env::var_os("WALLET_STUDIO_DEMO_DESIGNS_DIR") {
            let root=crate::storage::root().join("demo-library").join("pack");
            if std::fs::create_dir_all(&root).is_ok() {
                if let Ok(designs)=crate::design_library::discover(&PathBuf::from(source)) {
                    for (index,design) in designs.into_iter().take(crate::design_library::MAX_INCLUDED_DESIGNS).enumerate() {
                        if let Ok(bytes)=crate::design_library::read_image(&design.path) {
                            if let Ok(hash)=crate::bank_backup::sha256(&bytes) {
                                let item=crate::design_library::PurchasedDesign {id:format!("ws-{:03}",index+1),title:design.title,filename:format!("ws-{:03}.png",index+1),sha256:hash,bytes:bytes.len(),width:320,height:200};
                                if crate::design_library::verify_purchased(&item,&bytes).is_ok() {let path=root.join(&item.filename);if crate::storage::write_new(&path,&bytes).is_ok() {self.purchased_designs.push((item,path));}}
                            }
                        }
                    }
                }
            }
        }
        if std::env::args().any(|arg|arg=="--wallet-studio-library-demo") {self.current_tab=AppTab::Library;}

    }
    fn start_cloud_startup(&mut self,ctx:egui::Context) {
        let (tx,rx)=channel(); self.cloud_rx=Some(rx);
        let endpoint=self.account_server.clone();
        thread::spawn(move || {
            let _=tx.send(CloudEvent::Progress("Comprobando y preparando las copias locales…"));ctx.request_repaint();
            let mut message=CloudMessage { authenticated:false,library_authenticated:false,session_pending:false,storage_ready:false,configured:true,synced:false,server:String::new(),status:"No se pudo preparar el almacenamiento. Conserva las copias y revisa la recuperación.".into(),session:None };
            if crate::storage::migrate_legacy().is_ok() {
                message.storage_ready=true;
                message.configured=crate::cloud_sync::account_expected_without_session(&endpoint);
                match crate::cloud_sync::load_session() {
                    Ok(Some(session))=> { message.configured=true; message.server=session.server.clone();
                        let _=tx.send(CloudEvent::Restored(session.clone()));ctx.request_repaint();
                        (message.authenticated,message.library_authenticated,message.synced,message.session_pending,message.status)=cloud_sync_status(&session,false,&tx,&ctx);
                        message.session=Some(session);
                    }
                    Ok(None)=>message.status=if message.configured {"Inicia sesión y comprueba tus copias en la nube antes de capturar un original ausente. Los originales locales verificados siguen disponibles."} else {"Almacenamiento local listo · vincula tu cuenta para sincronizar"}.into(),
                    Err(_)=> { message.configured=true; message.status="La sesión no se pudo verificar. Inicia sesión de nuevo; no se capturará un original ausente.".into(); }
                }
            }
            if message.storage_ready { message.storage_ready=crate::storage::ensure_ready().is_ok(); }
            let _=tx.send(CloudEvent::Complete(message)); ctx.request_repaint();
        });
    }
    fn tick_cloud(&mut self,ctx:&egui::Context) {
        if crate::storage::is_demo() { return; }
        if let Some(result)=self.activation_rx.as_ref().and_then(|rx|rx.try_recv().ok()) {
            self.activation_rx=None;
            if result {self.account_stage=2;self.cloud_status="Si tu compra y solicitud son válidas, recibirás un código. Revisa también el correo no deseado.".into();}
            else {self.cloud_status="No se pudo enviar el código. El proveedor debe revisar el servicio y el correo de activación.".into();}
        }
        if self.activation_rx.is_some() {ctx.request_repaint_after(std::time::Duration::from_millis(150));}
        if let Some(result)=self.account_devices_rx.as_ref().and_then(|rx|rx.try_recv().ok()) {
            self.account_devices_rx=None;match result {Ok(devices)=>self.account_devices=devices,Err(())=>self.cloud_status="No se pudo actualizar la lista de equipos. Vuelve a comprobar la sesión.".into()}
        }
        if self.account_devices_rx.is_some() {ctx.request_repaint_after(std::time::Duration::from_millis(150));}
        loop {
            let event=match self.cloud_rx.as_ref().map(|rx|rx.try_recv()) {
                Some(Ok(event))=>event,
                Some(Err(std::sync::mpsc::TryRecvError::Disconnected))=>{
                    self.cloud_rx=None;self.cloud_synced=false;self.last_cloud_sync=Instant::now();self.cloud_sync_queued=false;
                    if self.cloud_session.is_some() {self.account_authenticated=false;self.library_authenticated=true;self.session_pending=true;}
                    self.cloud_status="La comprobación se interrumpió. La sesión guardada se conserva; reintenta la conexión desde Cuenta.".into();break;
                },
                _=>break,
            };
            let message=match event {
                CloudEvent::Progress(status)=>{self.cloud_status=status.into();continue;},
                CloudEvent::Restored(session)=>{
                    self.account_authenticated=false;self.library_authenticated=true;self.session_pending=true;
                    self.cloud_configured=true;self.cloud_synced=false;self.storage_ready=crate::storage::ensure_ready().is_ok();
                    self.account_server=session.server.clone();self.cloud_session=Some(session);
                    self.cloud_status="Sesión guardada en este equipo. Comprobando la conexión; puedes preparar tus diseños mientras esperas.".into();continue;
                },
                CloudEvent::Complete(message)=>message,
            };
            self.cloud_rx=None; self.storage_ready=message.storage_ready; self.cloud_configured=message.configured;
            self.cloud_synced=message.synced; self.cloud_status=message.status.into();
            let newly_authenticated=!self.account_authenticated && message.authenticated;
            self.account_authenticated=message.authenticated;self.library_authenticated=message.library_authenticated;
            self.session_pending=message.session_pending;
            if !message.server.is_empty() { self.account_server=message.server; }
            self.cloud_session=if !message.library_authenticated {
                if let Some(session)=message.session {let _=crate::cloud_sync::forget_rejected_session(&session);}
                None
            } else {message.session};
            self.last_cloud_sync=Instant::now();
            self.saved_cards=load_saved_cards(); self.invalidate_card_cache();
            if newly_authenticated && self.apple_ready {self.refresh_devices();self.refresh_recovery_status();}
        }
        if let Some(rx)=&self.update_rx { while let Ok(message)=rx.try_recv() { self.update_status=message; } }
        if self.cloud_rx.is_some() { ctx.request_repaint_after(std::time::Duration::from_millis(150)); }
        if self.cloud_configured && self.cloud_session.is_some() {
            if self.cloud_sync_queued || self.last_cloud_sync.elapsed().as_secs()>=60 { self.start_cloud_sync(ctx.clone()); }
            ctx.request_repaint_after(std::time::Duration::from_secs(60));
        }
    }
    fn start_cloud_sync(&mut self,ctx:egui::Context) {
        if crate::storage::is_demo() || self.is_busy || self.scanning_syslog || self.cloud_rx.is_some() { return; }
        let Some(session)=self.cloud_session.clone() else { return; };
        self.cloud_sync_queued=false; self.last_cloud_sync=Instant::now();
        self.cloud_synced=false;
        let previously_authenticated=self.account_authenticated;
        let (tx,rx)=channel(); self.cloud_rx=Some(rx);
        thread::spawn(move|| {
            let (authenticated,library_authenticated,synced,session_pending,status)=cloud_sync_status(&session,previously_authenticated,&tx,&ctx);
            let message=CloudMessage { authenticated,library_authenticated,session_pending,storage_ready:crate::storage::ensure_ready().is_ok(),configured:true,synced,
                server:session.server.clone(),session:Some(session),status };
            let _=tx.send(CloudEvent::Complete(message));ctx.request_repaint();
        });
    }
    fn show_account_tab(&mut self,ui:&mut egui::Ui,ctx:&egui::Context) {
        m3_card(ui,|ui| {
            crate::auth_ui::steps(ui,self.account_stage,self.account_recovery);
            ui.label(egui::RichText::new(if self.account_authenticated || self.library_authenticated {"Cuenta y copias"} else if self.account_stage==2 {if self.account_recovery {"Recupera tu acceso"} else {"Activa tu espacio"}} else if self.account_stage==1 {"Qué bien verte de nuevo"} else {"Tu estudio empieza aquí"}).size(27.0).strong());
            ui.add_space(12.0);
            crate::auth_ui::notice(ui,&self.cloud_status,self.cloud_rx.is_some() || self.activation_rx.is_some());
            ui.add_space(18.0);
            if self.account_authenticated || self.library_authenticated {
                ui.label("Sesión guardada de forma segura en este equipo. Se comprobará al abrir la app; caduca tras 30 días sin uso o a los 90 días.");
                ui.add_space(12.0);
                let downloaded=self.purchased_designs.len();
                let uploaded=self.library_designs.len();
                let verified=if self.cloud_synced {downloaded} else {0};
                ui.columns(3,|columns| {
                    kpi_card(&mut columns[0],downloaded,"Descargadas",md3::PRIMARY);
                    kpi_card(&mut columns[1],uploaded,"Subidas",md3::ON_SURFACE_VARIANT);
                    kpi_card(&mut columns[2],verified,"Verificadas",if verified>0 {md3::SUCCESS} else {md3::ON_SURFACE_VARIANT});
                });
                ui.add_space(8.0);
                let idle=self.account_devices_rx.is_none() && self.cloud_rx.is_none() && !self.is_busy && !self.scanning_syslog;
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(if self.cloud_synced {"Copias verificadas"} else {"Sincronización pendiente"}).strong().color(if self.cloud_synced {md3::SUCCESS} else {md3::PRIMARY}));
                });
                if studio_button(ui,if self.cloud_rx.is_some() {"Sincronizando…"} else {"Sincronizar ahora"},idle && (self.account_authenticated || self.session_pending),true) {self.start_cloud_sync(ctx.clone());}
                ui.add_space(16.0);
                let mut revoke=None;
                for (index,device) in self.account_devices.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(format!("Equipo {} · {}{}",index+1,&device.id[..8],if device.current {" · este PC"} else {""}));
                        let now=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();let days=now.saturating_sub(device.last_seen)/86400;
                        ui.label(if days==0 {"Último uso: hoy".to_string()} else {format!("Último uso: hace {days} días")});
                        if studio_button(ui,"Desvincular",idle && !device.current,false) {revoke=Some(device.id.clone());}
                    });
                }
                if studio_button(ui,"Ver / actualizar mis 3 equipos",idle,false) || revoke.is_some() {
                    if let Some(session)=self.cloud_session.clone() {
                        let (tx,rx)=channel();self.account_devices_rx=Some(rx);let ctx=ctx.clone();
                        thread::spawn(move|| {let result=(|| {if let Some(id)=revoke {crate::cloud_sync::revoke_computer(&session,&id)?;}crate::cloud_sync::registered_computers(&session)})();let _=tx.send(result.map_err(|_:anyhow::Error|()));ctx.request_repaint();});
                    }
                }
                if studio_button(ui,"Cerrar sesión en este PC",idle,false) {
                    if crate::cloud_sync::forget_session().is_ok() {self.cloud_session=None;self.account_authenticated=false;self.library_authenticated=false;self.session_pending=false;self.cloud_synced=false;self.account_devices.clear();self.account_stage=1;self.cloud_status="Sesión cerrada. Tus originales y su propietario se conservan.".into();}
                }
                return;
            }
            ui.label("Accede con el correo que utilizaste al comprar Wallet Studio.");
            if self.cloud_session.is_some() && self.cloud_rx.is_none() && !crate::storage::is_demo() {
                if studio_button(ui,"Reintentar conexión con mi sesión guardada",!self.is_busy && self.activation_rx.is_none(),false) {self.start_cloud_sync(ctx.clone());}
            }
            if !self.storage_ready && self.cloud_rx.is_none() && !crate::storage::is_demo() {
                ui.label("Si acabas de cerrar otra versión, puedes volver a comprobar el almacenamiento sin borrar ninguna copia.");
                if studio_button(ui,"Volver a comprobar mis copias locales",!self.is_busy && self.activation_rx.is_none(),false) {
                    let _ = std::fs::remove_file(crate::storage::root().join(".migration.lock"));
                    self.start_cloud_startup(ctx.clone());
                }
            }
            if self.account_stage==2 { ui.label("Introduce el código que te enviamos a ese correo y elige tu contraseña."); }
            if self.account_server.is_empty() && !self.auth_preview { ui.add_space(8.0); ui.label(egui::RichText::new("El servicio de cuentas aún no está configurado. El acceso estará disponible cuando se active. Tus copias locales se conservan.").color(md3::ON_SURFACE_VARIANT)); }
            egui::CollapsingHeader::new("Configuración avanzada del servicio").default_open(self.account_server.is_empty()).show(ui,|ui| {
                ui.label("Servicio de sincronización (HTTPS)");
                ui.add(egui::TextEdit::singleline(&mut self.account_server).hint_text("https://wallet-studio-accounts.onrender.com").desired_width(ui.available_width()));
            });
            ui.add_space(12.0); ui.label("Correo electrónico"); ui.add_enabled(self.account_stage!=2,egui::TextEdit::singleline(&mut self.account_email).hint_text("tu@correo.com").background_color(md3::SURFACE_CONTAINER_HIGH).margin(egui::vec2(12.0,10.0)).desired_width(ui.available_width())); ui.add_space(10.0);
            let available=!self.account_server.is_empty() && self.storage_ready && self.cloud_rx.is_none() && self.activation_rx.is_none() && !self.is_busy && !self.scanning_syslog && !crate::storage::is_demo();
            let login_available=!self.account_server.is_empty() && self.cloud_rx.is_none() && self.activation_rx.is_none() && !self.is_busy && !self.scanning_syslog && !crate::storage::is_demo();
            if !available && !self.auth_preview {
                let reason=if self.account_server.is_empty() {
                    "El servicio de cuentas no está configurado. Introduce la dirección en la configuración avanzada."
                } else if !self.storage_ready {
                    if self.account_stage == 1 {
                        "Almacenamiento local en revisión. Puedes iniciar sesión con tu cuenta."
                    } else {
                        "Preparando almacenamiento local de copias…"
                    }
                } else if self.cloud_rx.is_some() || self.activation_rx.is_some() || self.is_busy {
                    "Operación en curso, espera un momento…"
                } else if crate::storage::is_demo() {
                    "Modo demostración activo."
                } else {
                    "Servicio no disponible temporalmente."
                };
                ui.label(egui::RichText::new(reason).size(11.5).color(md3::ON_SURFACE_VARIANT));
            }
            if self.account_stage==0 {
                if studio_button(ui,"Activar mi acceso",available,true) {
                    self.account_recovery=false;self.activation_requested=Some(Instant::now());
                    let (tx,rx)=channel();self.activation_rx=Some(rx);let server=self.account_server.clone();let email=self.account_email.clone();let ctx=ctx.clone();
                    self.cloud_status="Solicitando código de activación…".into();
                    thread::spawn(move|| {let ok=crate::cloud_sync::start_activation(&server,&email).is_ok();let _=tx.send(ok);ctx.request_repaint();});
                }
                if studio_button(ui,"Iniciar sesión con mi contraseña",!self.is_busy,false) {self.account_stage=1;}
                if studio_button(ui,"He olvidado mi contraseña",available,false) {
                    self.account_recovery=true;self.activation_requested=Some(Instant::now());let (tx,rx)=channel();self.activation_rx=Some(rx);let server=self.account_server.clone();let email=self.account_email.clone();let ctx=ctx.clone();
                    self.cloud_status="Enviando código al correo de compra para recuperar el acceso…".into();
                    thread::spawn(move|| {let ok=crate::cloud_sync::start_email_code(&server,&email,true).is_ok();let _=tx.send(ok);ctx.request_repaint();});
                }
            } else {
                if self.account_stage==2 {ui.label("Código de verificación");ui.add(egui::TextEdit::singleline(&mut self.account_code).hint_text("6 cifras").background_color(md3::SURFACE_CONTAINER_HIGH).margin(egui::vec2(12.0,10.0)).desired_width(ui.available_width()));ui.add_space(10.0);
                    let now=Instant::now();
                    let cooldown=crate::auth_ui::remaining(self.activation_requested,now,crate::auth_ui::RESEND_COOLDOWN);
                    let lifetime=crate::auth_ui::remaining(self.activation_requested,now,crate::auth_ui::CODE_LIFETIME);
                    ui.label(egui::RichText::new(if self.activation_requested.is_none() {"El código caduca 10 minutos después de enviarse.".into()} else if lifetime==0 {"Han pasado 10 min desde la solicitud. Si el código no funciona, solicita uno nuevo.".into()} else {format!("Quedan aprox. {:02}:{:02} desde la solicitud · utiliza el más reciente",lifetime/60,lifetime%60)}).size(12.0).color(md3::PRIMARY));
                    let label=if cooldown>0 {format!("Reenviar código en {cooldown} s")} else {"Reenviar código de activación".into()};
                    if studio_button(ui,&label,available && cooldown==0,false) {
                        self.account_code.clear();self.activation_requested=Some(now);
                        let (tx,rx)=channel();self.activation_rx=Some(rx);
                        let server=self.account_server.clone();let email=self.account_email.clone();let recovery=self.account_recovery;let ctx=ctx.clone();
                        self.cloud_status="Solicitando un nuevo código. El anterior dejará de ser válido si se envía uno nuevo…".into();
                        thread::spawn(move|| {let ok=crate::cloud_sync::start_email_code(&server,&email,recovery).is_ok();let _=tx.send(ok);ctx.request_repaint();});
                    }
                    if self.activation_requested.is_some() {ctx.request_repaint_after(std::time::Duration::from_secs(1));}
                }
                ui.label("Contraseña (12–128 caracteres)");ui.add(egui::TextEdit::singleline(&mut self.account_password).password(!self.password_visible).background_color(md3::SURFACE_CONTAINER_HIGH).margin(egui::vec2(12.0,10.0)).desired_width(ui.available_width()));ui.add_space(10.0);
                if self.account_stage==1 && !self.account_password.is_empty() && self.account_password.len()<12 {
                    ui.colored_label(md3::ERROR,"La contraseña debe tener al menos 12 caracteres.");
                }
                if self.account_stage==2 {ui.label("Repite la contraseña");ui.add(egui::TextEdit::singleline(&mut self.account_confirmation).password(!self.password_visible).background_color(md3::SURFACE_CONTAINER_HIGH).margin(egui::vec2(12.0,10.0)).desired_width(ui.available_width()));ui.add_space(10.0);}
                ui.checkbox(&mut self.password_visible,"Mostrar contraseña");
                if self.account_stage==2 && !self.account_confirmation.is_empty() && self.account_password!=self.account_confirmation {ui.colored_label(md3::ERROR,"Las contraseñas todavía no coinciden.");}
                if self.account_stage==2 && self.account_recovery {ui.checkbox(&mut self.account_reset_devices,"Desvincular mis equipos anteriores y registrar este PC (requiere el código del correo). Sus sesiones se cerrarán; sus copias locales se conservan.");}
            }
            if self.account_stage>0 {ui.checkbox(&mut self.account_link_consent,"Vincular las copias de este perfil a mi cuenta").on_hover_text("Confirmo que todas las copias e historiales de este perfil me pertenecen. Se sincronizarán con esta cuenta; su propietario no podrá cambiarse desde esta instalación.");}
            let activation=self.account_stage==2;
            let matching=if activation {
                self.account_password==self.account_confirmation && (12..=128).contains(&self.account_password.chars().count()) && self.account_code.len()==6 && self.account_code.bytes().all(|b|b.is_ascii_digit())
            } else {
                (12..=128).contains(&self.account_password.chars().count())
            };
            let can_submit=if activation {available} else {login_available};
            let action=if self.account_stage>0 && studio_button(ui,if activation {"Verificar y crear mi cuenta"} else {"Iniciar sesión"},can_submit && matching,true) {Some(activation)} else {None};
            if let Some(register)=action {
                let server=self.account_server.clone(); let email=self.account_email.clone(); let password=std::mem::take(&mut self.account_password);
                let confirmation=std::mem::take(&mut self.account_confirmation);let code=std::mem::take(&mut self.account_code);
                let recovery=self.account_recovery;let reset_devices=self.account_reset_devices;
                let consent=self.account_link_consent;
                let old_configured=self.cloud_configured; let (tx,rx)=channel(); self.cloud_rx=Some(rx); self.cloud_synced=false; self.storage_ready=false;
                self.cloud_status="Verificando cuenta y originales…".into(); let ctx=ctx.clone();
                thread::spawn(move || {
                    let mut message=CloudMessage { authenticated:false,library_authenticated:false,session_pending:false,storage_ready:true,configured:old_configured,synced:false,server:server.clone(),status:"Acceso rechazado: revisa compra, código, contraseña y límite de 3 equipos.".into(),session:None };
                    let result=if register && recovery {crate::cloud_sync::recover_account(&server,&email,&code,&password,&confirmation,reset_devices,consent)} else if register {crate::cloud_sync::activate(&server,&email,&code,&password,&confirmation,consent)} else {crate::cloud_sync::login(&server,&email,&password,false,consent)};
                    match result {
                      Ok(session)=> {
                        message.configured=true;
                        (message.authenticated,message.library_authenticated,message.synced,message.session_pending,message.status)=cloud_sync_status(&session,true,&tx,&ctx);
                        message.session=Some(session);
                      }
                      Err(error)=>message.status=crate::cloud_sync::account_failure_message(&error).into(),
                    }
                    message.configured|=crate::cloud_sync::account_expected_without_session(&server);
                    message.storage_ready=crate::storage::ensure_ready().is_ok();
                    let _=tx.send(CloudEvent::Complete(message)); ctx.request_repaint();
                });
            }
            if self.account_stage>0 && studio_button(ui,"Volver al correo",!self.is_busy,false) {self.account_stage=0;self.activation_requested=None;self.password_visible=false;self.account_password.clear();self.account_confirmation.clear();self.account_code.clear();}
            ui.add_space(12.0);
            egui::CollapsingHeader::new("Sobre tu cuenta y tus copias").show(ui,|ui| {
                ui.label("La nube conserva originales bancarios verificados e historial por iPhone y tarjeta. Las copias del formato antiguo y los diarios inciertos se conservan localmente.");
                ui.label("El propietario de las copias no se cambia al iniciar sesión con otra cuenta. Solo se captura un original ausente después de verificar la nube y su procedencia.");
                ui.label(&self.update_status);
            });
        });
    }
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
        let brand=crate::brand::BrandLogo::new(&cc.egui_ctx);

        let (apple_ready, apple_status) = if crate::storage::is_demo() { (false,"Demostración aislada: sin consultas USB".into()) } else { match apple::verify_support() {
            Ok(msg) => (true, msg),
            Err(err) => (false, err.to_string()),
        }};

        let language = Language::load();
        let saved_cards = load_saved_cards();
        let offline_baseline_device = HashMap::new();
        let mut app = Self {
            activation_requested:None,password_visible:false,activation_rx:None,account_stage:0,account_code:String::new(),account_confirmation:String::new(),account_authenticated:false,library_authenticated:false,session_pending:false,
            account_devices:Vec::new(),account_devices_rx:None,
            account_recovery:false,account_reset_devices:false,
            auth_preview:crate::storage::is_demo() && (std::env::args().any(|arg|arg=="--wallet-studio-auth-demo") || std::env::current_exe().ok().and_then(|path|path.file_stem().map(|stem|matches!(stem.to_string_lossy().to_ascii_uppercase().as_str(),"WSDEMOAU"|"WSDEMOA9"|"WSDEMOAF"))).unwrap_or(false)),
            cloud_rx:None,cloud_session:None,cloud_sync_queued:false,last_cloud_sync:Instant::now(),storage_ready:false,cloud_configured:true,cloud_synced:false,
            cloud_status:"Preparando almacenamiento seguro…".into(),account_server:option_env!("WALLET_STUDIO_CLOUD_URL").unwrap_or("https://wallet-studio-accounts.onrender.com").into(),account_email:String::new(),account_password:String::new(),account_link_consent:false,
            update_rx:None,update_status:"Actualizaciones automáticas".into(),
            brand,
            current_tab: AppTab::Wallet,
            language,
            apple_status,
            apple_ready,

            devices: Vec::new(),
            selected_udid: None,
            connection_mode: ConnectionMode::Auto,

            card_hash: String::new(),
            saved_cards,
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
            library_selection: crate::library_loader::Selection::default(),
            library_error: None,
            library_textures:HashMap::new(),purchased_designs:Vec::new(),purchased_status:"Wallet Studio incluye una biblioteca de diseños. Actualízala para consultarlos.".into(),purchased_rx:None,purchased_use_rx:None,library_owner:String::new(),library_previews:crate::library_loader::Previews::default(),
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
            recovery_status_error: None,
        };

        app.add_log("Wallet Studio initialized");
        app.add_log(format!("Apple Support Runtime: {}", if app.apple_ready { "Loaded and operational" } else { "Not found (iTunes required)" }));
        app.add_log(format!("Loaded {} saved card(s) from database", app.saved_cards.len()));

        if !crate::storage::is_demo() { app.update_rx=Some(crate::auto_update::start_background_check()); }
        if !crate::storage::is_demo() { app.start_cloud_startup(cc.egui_ctx.clone()); }
        // No device query, recovery mutation or artwork capture before purchaser authentication.
        app.refresh_design_library();

        if crate::storage::is_demo() { app.seed_demo(&cc.egui_ctx); }
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
        if crate::storage::is_demo() { return; }
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
        match crate::design_library::library_dir().and_then(|root| {if !crate::storage::is_demo() {crate::design_library::migrate_legacy_saves(&root)?;}crate::design_library::discover(&root)}) {
            Ok(designs) => {
                if !designs.iter().any(|design| Some(&design.path) == self.library_selection.path.as_ref())
                    && !self.purchased_designs.iter().any(|(_,path)| Some(path) == self.library_selection.path.as_ref()) {
                    self.library_selection.select(None);
                }
                self.library_designs = designs;
                self.library_textures.retain(|path,_|self.library_designs.iter().any(|design|&design.path==path) || self.purchased_designs.iter().any(|(_,paid)|paid==path));
                self.library_error = None;
            }
            Err(_) => {
                self.library_designs.clear();
                self.library_selection.select(None);
                self.library_error = Some("No se pudo abrir tu biblioteca. Inicia sesión y revisa el almacenamiento personal.".into());
            }
        }
    }

    fn start_purchased_refresh(&mut self,ctx:&egui::Context) {
        if crate::storage::is_demo() || self.purchased_rx.is_some() {return;}
        let owner=self.library_owner.clone();
                if let Some(session)=self.cloud_session.clone() {
                    self.library_previews.reset();
                    self.purchased_status="Consultando compras y preparando miniaturas…".into();
                    let (tx,rx)=std::sync::mpsc::channel();self.purchased_rx=Some(rx);let key=owner.clone();let ctx=ctx.clone();
                    std::thread::spawn(move|| {let result=(||->anyhow::Result<_> {let catalog=crate::design_library::purchased_catalog(&session)?;catalog.designs.into_iter().map(|item| {let path=crate::design_library::purchased_cache_path(&session,&item)?;Ok((item,path))}).collect()})();let _=tx.send((key,result.map_err(|_|"No se pudo actualizar el pack. Revisa conexión y compra; el servidor puede requerir configuración o actualización.".into())));ctx.request_repaint();});
                }
    }
    fn show_library_tab(&mut self,ctx:&egui::Context,ui:&mut egui::Ui) {
        let owner=if crate::storage::is_demo() {"demo".into()} else {self.cloud_session.as_ref().map(|s|format!("{}|{}",s.server,s.user_id)).unwrap_or_default()};
        if owner!=self.library_owner {
            self.library_owner=owner.clone();self.library_designs.clear();self.library_textures.clear();self.library_selection.reset();self.purchased_designs.clear();self.purchased_status="Wallet Studio incluye una biblioteca de diseños. Actualízala para consultarlos.".into();self.purchased_rx=None;self.purchased_use_rx=None;self.library_previews.reset();self.refresh_design_library();
            self.start_purchased_refresh(ctx);
        }
        match self.purchased_rx.as_ref().map(|rx|rx.try_recv()) {
            Some(Ok((key,result)))=>{
                self.purchased_rx=None;
                if key==owner {match result {Ok(items)=> {
                    let native_2k=items.len()==56 && items.iter().all(|(item,_)| (item.width,item.height)==(crate::image_skin::CARD_WIDTH,crate::image_skin::CARD_HEIGHT));
                    self.purchased_status=if native_2k {"56 diseños 2K verificados (1536 × 969). Se priorizan originales locales y se rechazan miniaturas obsoletas.".into()} else if items.is_empty() {"Tu compra de Wallet Studio incluye diseños. Si no aparecen, actualiza la biblioteca o revisa el estado de tu cuenta.".into()} else {format!("{} diseños incluidos con Wallet Studio. Se verifica tu acceso y la resolución al usar un diseño.",items.len())};
                    self.purchased_designs=items;
                },Err(error)=>{self.purchased_status=error;}}}
            },
            Some(Err(std::sync::mpsc::TryRecvError::Disconnected))=>{self.purchased_rx=None;self.purchased_status="No se pudo actualizar la biblioteca. Vuelve a intentarlo.".into();},
            _=>{},
        }
        // A removed paid item must never fall through to the local-image route.
        if self.library_selection.path.as_ref().is_some_and(|selected|
            !self.library_designs.iter().any(|item|&item.path==selected)
            && !self.purchased_designs.iter().any(|(_,path)|path==selected)) {
            self.library_selection.select(None);
        }
        if !self.is_busy {match self.purchased_use_rx.as_ref().map(|rx|rx.try_recv()) {
            Some(Ok((key,request,result)))=>{
                self.purchased_use_rx=None;
                if key==owner && self.library_selection.finish(request) {match result {
                    Ok(loaded)=>{self.install_library_design(ctx,loaded);self.library_error=None;self.current_tab=AppTab::Wallet;ui.scroll_to_cursor(Some(egui::Align::Min));ctx.request_repaint();return;},
                    Err(error)=>self.library_error=Some(error),
                }}
            },
            Some(Err(std::sync::mpsc::TryRecvError::Disconnected))=>{self.purchased_use_rx=None;self.library_selection.cancel();self.library_error=Some("La preparación se interrumpió. Tu selección se conserva; vuelve a intentarlo.".into());},
            _=>{},
        }}
        for (path,color) in self.library_previews.poll() {
            self.library_textures.insert(path,ctx.load_texture("library-preview",color,egui::TextureOptions::LINEAR));
        }
        ui.heading("Biblioteca");ui.label("Tus diseños guardados y los diseños incluidos con Wallet Studio.");ui.add_space(12.0);
        let idle=!self.is_busy && !self.library_selection.busy();
        ui.horizontal_wrapped(|ui| {
            if studio_button(ui,"Importar imagen…",idle,false) {
                if let Some(path)=rfd::FileDialog::new().add_filter("Imagen", &["png","jpg","jpeg","webp","gif"]).pick_file() {
                    let result=(||->anyhow::Result<PathBuf> {
                        let bytes=crate::design_library::read_image(&path)?;
                        let image=crate::design_library::decode_still(&bytes)?;
                        let prepared=PreparedSkin::from_image_with_focus(image,0.5,0.5)?;
                        crate::design_library::save_prepared_png(&crate::design_library::library_dir()?,&prepared.png)
                    })();
                    match result {Ok(saved)=>{self.load_skin_path(ctx,saved);self.refresh_design_library();},Err(_)=>self.library_error=Some("No se pudo importar la imagen. Usa un archivo PNG, JPG, WebP o GIF válido de hasta 32 MB.".into())}
                }
            }
            if studio_button(ui,"Guardar diseño actual",idle && self.skin.is_some(),false) {
                if let Some(skin)=&self.skin {match crate::design_library::library_dir().and_then(|root|crate::design_library::save_prepared_png(&root,&skin.png)) {Ok(_)=>self.refresh_design_library(),Err(_)=>self.library_error=Some("No se pudo guardar el diseño.".into())}}
            }
            if studio_button(ui,"Actualizar diseños incluidos",idle && self.purchased_rx.is_none() && self.cloud_session.is_some(),false) {
                self.start_purchased_refresh(ctx);
            }
        });
        if let Some(error)=&self.library_error {ui.colored_label(md3::ERROR,error);}
        let selection_top=ui.cursor().min;
        self.library_selection_panel(ctx,ui);
        ui.add_space(12.0);ui.label(egui::RichText::new("TUS DISEÑOS").strong());
        if self.library_designs.is_empty() {ui.label("Todavía no tienes diseños guardados. Importa una imagen o guarda tu diseño actual.");}
        let own=self.library_designs.clone();self.library_grid(ctx,ui,own.into_iter().map(|d|(d.title,d.path,None)).collect(),selection_top);
        ui.add_space(20.0);ui.label(egui::RichText::new(if self.purchased_designs.is_empty() {"DISEÑOS INCLUIDOS".into()} else {format!("{} DISEÑOS INCLUIDOS",self.purchased_designs.len())}).strong());ui.label(&self.purchased_status);
        if self.purchased_rx.is_some() || self.purchased_use_rx.is_some() || self.library_previews.busy() {ctx.request_repaint_after(std::time::Duration::from_millis(100));}
        let paid=self.purchased_designs.iter().map(|(item,path)|(item.title.clone(),path.clone(),Some(item.clone()))).collect();self.library_grid(ctx,ui,paid,selection_top);
    }
    fn request_library_preview(&mut self,ctx:&egui::Context,path:&Path,paid:Option<crate::design_library::PurchasedDesign>) {
        if self.library_textures.contains_key(path) {return;}
        let path=path.to_owned();let read_path=path.clone();let session=self.cloud_session.clone();let demo=crate::storage::is_demo();
        self.library_previews.request(path,ctx,move|| {
            if !demo {if let Some(item)=paid {crate::design_library::cached_purchased(&session.context("Inicia sesión para ver el pack")?,&item)?;}}
            let image=crate::design_library::read_image(&read_path).and_then(|bytes|crate::design_library::decode_still(&bytes))?;
            let image=image.thumbnail(image.width().min(640),image.height().min(400)).to_rgba8();
            Ok(egui::ColorImage::from_rgba_unmultiplied([image.width() as usize,image.height() as usize],image.as_raw()))
        });
    }
    fn library_selection_panel(&mut self,ctx:&egui::Context,ui:&mut egui::Ui) {
        let selected=self.library_selection.path.clone();
        m3_card(ui,|ui| {
            ui.label(egui::RichText::new("DISEÑO SELECCIONADO").strong());
            if let Some(path)=selected {
                let paid=self.purchased_designs.iter().find(|(_,item)|item==&path).map(|(item,_)|item.clone());
                let title=paid.as_ref().map(|item|item.title.clone()).or_else(||self.library_designs.iter().find(|item|item.path==path).map(|item|item.title.clone())).unwrap_or_else(||"Tu diseño".into());
                self.request_library_preview(ctx,&path,paid.clone());
                let width=ui.available_width().min(480.0);let size=egui::vec2(width,width*0.625);
                let (rect,_)=ui.allocate_exact_size(size,egui::Sense::hover());
                paint_library_preview(ui,rect,&path,self.library_textures.get(&path),self.library_previews.failed(&path));
                ui.label(title);
                if self.library_previews.failed(&path) {
                    ui.colored_label(md3::ERROR,if self.library_previews.exhausted(&path) {"No se pudo cargar la vista previa. Comprueba tu conexión y pulsa Reintentar imagen."} else {"No se pudo cargar la vista previa. Se reintentará automáticamente."});
                    if studio_button(ui,"Reintentar imagen",!self.is_busy && !self.library_selection.busy(),false) {self.library_previews.retry(&path);}
                    if !self.library_previews.exhausted(&path) {ctx.request_repaint_after(std::time::Duration::from_secs(1));}
                }
                if studio_button(ui,if self.library_selection.busy() {"Preparando diseño…"} else {"Usar diseño seleccionado"},!self.is_busy && !self.library_selection.busy(),true) {
                    if let Some(request)=self.library_selection.begin() {
                        self.library_error=None;
                        let (tx,rx)=channel();self.purchased_use_rx=Some(rx);
                        let session=self.cloud_session.clone();let key=self.library_owner.clone();let ctx=ctx.clone();let demo=crate::storage::is_demo();
                        thread::spawn(move|| {
                            let result=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||->anyhow::Result<_> {
                                let path=if !demo {if let Some(item)=paid {crate::design_library::cached_purchased(&session.context("Inicia sesión para usar el diseño")?,&item)?} else {path}} else {path};
                                prepare_library_design(path)
                            })).unwrap_or_else(|_|Err(anyhow::anyhow!("Preparación interrumpida"))).map_err(|_|"No se pudo preparar el diseño. Revisa la conexión, tu acceso y el archivo; tu selección y el diseño anterior se conservan. Vuelve a intentarlo.".into());
                            let _=tx.send((key,request,result));ctx.request_repaint();
                        });
                    }
                }
            } else {
                ui.label("Selecciona una imagen para verla ampliada y llevarla al editor de tarjetas.");
                studio_button(ui,"Usar diseño seleccionado",false,false);
            }
        });
    }
    fn library_grid(&mut self,ctx:&egui::Context,ui:&mut egui::Ui,items:Vec<(String,PathBuf,Option<crate::design_library::PurchasedDesign>)>,selection_top:egui::Pos2) {
        let columns=((ui.available_width()/184.0).floor() as usize).max(1);
        for row in items.chunks(columns) {ui.horizontal(|ui| {for (title,path,paid) in row {
            ui.vertical(|ui| {ui.set_width(172.0);
                let visible=ui.is_rect_visible(egui::Rect::from_min_size(ui.cursor().min,egui::vec2(172.0,150.0)));
                if visible {self.request_library_preview(ctx,path,paid.clone());}
                let selected=self.library_selection.path.as_ref()==Some(path);
                let (rect,response)=ui.allocate_exact_size(egui::vec2(168.0,105.0),egui::Sense::click());
                paint_library_preview(ui,rect,path,self.library_textures.get(path),self.library_previews.failed(path));
                if selected {ui.painter().rect_stroke(rect.expand(2.0),9.0,egui::Stroke::new(2.0,md3::PRIMARY),egui::StrokeKind::Outside);}
                if response.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() && !self.is_busy && !self.library_selection.busy() {
                    self.library_selection.select(Some(path.clone()));self.library_error=None;
                    ui.scroll_to_rect(egui::Rect::from_min_size(selection_top,egui::vec2(1.0,1.0)),Some(egui::Align::Min));
                }
                ui.add(egui::Label::new(title).truncate());
                if visible && self.library_previews.failed(path) && !self.library_previews.exhausted(path) {ctx.request_repaint_after(std::time::Duration::from_secs(1));}
            });
        }});ui.add_space(12.0);}
    }

    fn install_library_design(&mut self,ctx:&egui::Context,loaded:LoadedLibraryDesign) {
        self.source_texture=Some(ctx.load_texture("card-skin-source-preview",loaded.preview,egui::TextureOptions::LINEAR));
        self.status_msg="Diseño preparado. Selecciona tu tarjeta y revisa la vista previa antes de aplicarlo.".into();
        self.source_path=Some(loaded.path);self.source_image=Some(loaded.source_image);self.gif_frames=loaded.frames;
        self.gif_frame=0;self.gif_playback=None;self.crop_focus=[0.5,0.5];self.crop_zoom=1.0;
        self.crop_edges=None;self.crop_draft=None;self.crop_dirty=false;self.skin=Some(loaded.skin);
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
        if crate::storage::is_demo() {return;}
        if self.scanning_syslog {
            if let Some(flag) = self.scan_stop_flag.take() {
                flag.store(true, Ordering::Relaxed);
            }
            // Busy remains true until the worker reports that its session closed.
            self.progress_msg="Cerrando la búsqueda y liberando la conexión…".into();
            self.add_log("Syslog stop requested by user; waiting for receiver release.");
            self.status_msg = self.language.text("Syslog scanning stopped.").to_string();
            return;
        }

        if self.session_pending {self.status_msg="Sesión guardada: espera a que se verifique la conexión antes de buscar tarjetas.".into();return;}

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
                move |card_udid, hash, name| {
                    let _ = tx_card.send(BackgroundTaskMessage::CardFound { udid: card_udid, hash, name });
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
        if self.session_pending {self.status_msg="Sesión guardada: verifica la conexión desde Cuenta antes de aplicar un diseño.".into();return;}
        if crate::storage::is_demo() {return;}
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
        if crate::storage::is_demo() {return;}
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
        if self.cloud_rx.is_some() { return; }
        if !self.storage_ready || crate::storage::is_demo() { return; }
        if self.is_busy || self.scanning_syslog || self.pending_recovery_blocks_selected() || !self.selected_usb_ready() {
            return;
        }
        let Some(udid) = self.selected_udid.clone() else { return };
        let hash = self.card_hash.trim().to_string();
        if hash.is_empty() {
            self.status_msg = self.language.text("Selecciona una tarjeta antes de guardar su copia original.").to_string();
            return;
        }
        if let Some(request) = self.auto_capture.as_mut() {
            if request.hash != hash || request.device.as_deref() != Some(udid.as_str())
                || !matches!(request.phase, CapturePhase::Waiting | CapturePhase::Running) {
                return;
            }
            request.phase = CapturePhase::Running;
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
                crate::storage::ensure_card_ready(&udid, &hash)?;
                anyhow::ensure!(!crate::wallet_export::pending_recovery_blocks_device(
                    Ok(&crate::wallet_export::pending_recovery_operations()?),Some(&udid)),
                    "Resuelve la recuperación pendiente de este iPhone antes de capturar");
                if crate::bank_backup::load_baseline(&udid,&hash)?.is_some() { return Ok(Some("verified".into())); }
                if crate::wallet_backup::backup_evidence_exists(&udid,&hash) {
                    crate::wallet_backup::load_original_assets(&udid,&hash)?;
                    return Ok(Some("verified".into()));
                }
                anyhow::ensure!(crate::bank_backup::verified_baseline_device_for_card(&hash)
                    .is_none_or(|known| known == udid),
                    "Existe un original de otro iPhone; no se reutilizará como autorización de captura");
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
        self.check_recovery_status(true);
    }

    // Completion checks must never turn a failed capture into an automatic retry.
    fn check_recovery_status(&mut self, retry_capture: bool) {
        match crate::wallet_export::pending_recovery_operations() {
            Ok(pending) => {
                let count = pending.len();
                self.pending_recoveries = pending;
                self.recovery_status_error = None;
                if count == 0 {
                    self.add_log("Comprobación de recuperaciones: ninguna operación pendiente.");
                    if retry_capture {
                        self.status_msg = "Sin recuperaciones pendientes. Comprobando la copia original seleccionada…".into();
                    }
                } else {
                    self.add_log(format!("Comprobación de recuperaciones: {count} operación(es) pendiente(s)."));
                    self.status_msg = format!("Se encontraron {count} recuperación(es) pendiente(s). Revisa Registro.");
                }
            }
            Err(e) => {
                let err = format!("{:#}", e);
                self.add_log(format!("Error al comprobar recuperaciones: {err}"));
                self.recovery_status_error = Some(err);
            }
        }
        self.invalidate_card_cache();
        if retry_capture {
            if let Some(capture) = &mut self.auto_capture {
                capture.retry();
            }
            if !self.card_hash.is_empty() && !self.original_verified(&self.card_hash) && self.selected_usb_ready() && !self.is_busy {
                self.capture_original_backup();
            }
        }
    }

    fn recover_pending_original(&mut self, operation_dir: PathBuf) {
        if crate::storage::is_demo() {return;}
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

    fn prepare_pending_baseline(&mut self, operation_dir: PathBuf) {
        if crate::storage::is_demo() { return; }
        let Some(udid) = self.selected_udid.clone() else { return };
        self.is_busy = true;
        let (tx, rx) = channel(); self.task_rx = Some(rx);
        thread::spawn(move || {
            let result = crate::wallet_export::prepare_pending_from_baseline(&operation_dir, &udid)
                .map(|_| "Copia original verificada preparada en el PC. Pulsa Recuperar original pendiente para devolverla al iPhone y comprobarla.".into())
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
        if self.session_pending {self.status_msg="Sesión guardada: verifica la conexión desde Cuenta antes de aplicar un teclado.".into();return;}
        if crate::storage::is_demo() {return;}
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
        let mut card_found = false;
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
                BackgroundTaskMessage::CardFound { udid, hash, name } => {
                    card_found = true;
                    if let Some(flag)=self.scan_stop_flag.as_ref() {flag.store(true,Ordering::Release);}
                    self.scanning_syslog=false;
                    self.is_busy=false;
                    self.progress_msg="Tarjeta detectada. Guardando copia original automáticamente…".into();
                    self.selected_udid = Some(udid.clone());
                    if !self.devices.iter().any(|d| d.udid.eq_ignore_ascii_case(&udid)) {
                        match list_connected_devices() {
                            Ok(devices) => self.devices = devices,
                            Err(error) => self.add_log(format!("No se pudo comprobar el transporte del iPhone detectado: {error:#}")),
                        }
                    }
                    self.saved_cards = load_saved_cards();
                    self.invalidate_card_cache();
                    self.select_saved_card(&hash);
                    let display_name = self.saved_cards.iter()
                        .find(|card| card.hash == hash)
                        .map(|card| card.name.clone())
                        .unwrap_or(name);
                    self.add_log(format!("Card detected: {display_name} ({hash}) on iPhone ({udid})"));
                    self.status_msg = format!("{}: {} · guardando copia original…", self.language.text("Card detected"), display_name);

                    // Start only after this scanner batch and its channel are retired.
                }
                BackgroundTaskMessage::DesignSaved(path) => {
                    self.refresh_design_library();
                    // A reused older design may be outside the 100 most recent entries.
                    if !self.library_designs.iter().any(|design| design.path == path) {
                        if self.library_designs.len() >= crate::design_library::MAX_DESIGNS { self.library_designs.pop(); }
                        self.library_designs.push(crate::design_library::Design { path: path.clone(), title: "Diseño guardado automáticamente".into() });
                    }
                    self.library_selection.select(Some(path));
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
                            self.cloud_sync_queued=true;
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
                    self.scan_stop_flag=None;
                    finished = true;
                    match res {
                        Ok(ok_msg) => {
                            self.cloud_sync_queued=true;
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
        if retire_task_receiver(&mut self.task_rx, finished, card_found) {
            if card_found { self.scan_stop_flag = None; }
            self.check_recovery_status(card_found);
        }
    }
}

pub mod md3 {
    use eframe::egui::Color32;

    // M3 Dark scheme
    pub const SURFACE: Color32 = Color32::from_rgb(8, 11, 18);
    pub const SURFACE_CONTAINER: Color32 = Color32::from_rgb(17, 24, 39);
    pub const SURFACE_CONTAINER_HIGH: Color32 = Color32::from_rgb(24, 34, 53);
    pub const SURFACE_CONTAINER_HIGHEST: Color32 = Color32::from_rgb(31, 46, 70);
    pub const ON_SURFACE: Color32 = Color32::from_rgb(248, 250, 252);
    pub const ON_SURFACE_VARIANT: Color32 = Color32::from_rgb(148, 163, 184);
    pub const OUTLINE: Color32 = Color32::from_rgb(100, 116, 139);
    pub const OUTLINE_VARIANT: Color32 = Color32::from_rgb(42, 54, 75);

    // Primary
    pub const PRIMARY: Color32 = Color32::from_rgb(56, 189, 248);
    pub const ON_PRIMARY: Color32 = Color32::from_rgb(8, 11, 18);
    pub const PRIMARY_CONTAINER: Color32 = Color32::from_rgb(22, 61, 91);
    pub const ON_PRIMARY_CONTAINER: Color32 = Color32::from_rgb(186, 230, 253);

    // Secondary
    pub const SECONDARY_CONTAINER: Color32 = Color32::from_rgb(30, 41, 59);
    pub const ON_SECONDARY_CONTAINER: Color32 = Color32::from_rgb(226, 232, 240);

    // Tertiary
    pub const TERTIARY_CONTAINER: Color32 = Color32::from_rgb(69, 35, 54);
    pub const ON_TERTIARY_CONTAINER: Color32 = Color32::from_rgb(254, 205, 211);

    // Error
    pub const ERROR: Color32 = Color32::from_rgb(239, 68, 68);
    pub const ERROR_CONTAINER: Color32 = Color32::from_rgb(78, 25, 35);

    // Extra
    pub const SUCCESS: Color32 = Color32::from_rgb(16, 185, 129);
}

fn draw_status_dot(ui: &mut egui::Ui, color: egui::Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), 4.0, color);
}

fn draw_nav_icon(ui: &mut egui::Ui, index: usize, active: bool) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(22.0, 22.0), egui::Sense::hover());
    let color = if active { md3::PRIMARY } else { md3::ON_SURFACE_VARIANT };
    let p = ui.painter();
    match index {
        0 => { p.rect_stroke(rect.shrink(4.0), 4.0, egui::Stroke::new(1.6, color), egui::StrokeKind::Inside); p.line_segment([rect.left_center() + egui::vec2(5.0,0.0), rect.right_center() - egui::vec2(5.0,0.0)], egui::Stroke::new(1.4,color)); }
        1 => { for i in 0..2 { for j in 0..2 { let r=egui::Rect::from_min_size(rect.min+egui::vec2(4.0+i as f32*8.0,4.0+j as f32*8.0),egui::vec2(5.0,5.0)); p.rect_filled(r,1.5,color); } } }
        2 => { p.circle_stroke(rect.center(), 8.0, egui::Stroke::new(1.5,color)); p.circle_filled(rect.center(), 2.5, color); }
        3 => { p.circle_stroke(rect.center(), 8.0, egui::Stroke::new(1.5,color)); p.circle_filled(rect.center(), 3.0, color); }
        _ => { p.circle_stroke(rect.center(), 8.0, egui::Stroke::new(1.5,color)); p.line_segment([rect.center()-egui::vec2(4.0,0.0),rect.center()+egui::vec2(4.0,0.0)],egui::Stroke::new(1.4,color)); }
    }
}

fn setup_custom_fonts(ctx: &egui::Context) {
    // Bundled egui fonts provide identical Spanish/Latin metrics on every PC.
    ctx.set_fonts(egui::FontDefinitions::default());
}

fn setup_custom_theme(ctx: &egui::Context) {
    // Keep the branded palette independent of the Windows light/dark setting.
    ctx.set_theme(egui::ThemePreference::Dark);
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
    visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0_f32,md3::OUTLINE_VARIANT);
    visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0_f32, md3::ON_SURFACE_VARIANT);
    visuals.widgets.inactive.corner_radius = 12.into();

    visuals.widgets.hovered.bg_fill = md3::SURFACE_CONTAINER_HIGHEST;
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0_f32,md3::PRIMARY);
    visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0_f32, md3::ON_SURFACE);
    visuals.widgets.hovered.corner_radius = 12.into();

    visuals.widgets.active.bg_fill = md3::PRIMARY_CONTAINER;
    visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0_f32,md3::PRIMARY);
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

fn glass_panel<R>(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::new()
        .fill(md3::SURFACE_CONTAINER.linear_multiply(0.86))
        .stroke(egui::Stroke::new(1.0, md3::OUTLINE_VARIANT))
        .corner_radius(14)
        .inner_margin(egui::Margin::same(16))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.vertical(add_contents).inner
        })
        .inner
}

fn glass_card<R>(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    glass_panel(ui, add_contents)
}

fn m3_card<R>(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    glass_card(ui, add_contents)
}

fn gradient_button_with_width(ui: &mut egui::Ui, label: &str, enabled: bool, primary: bool, width: f32) -> bool {
    let height = 44.0;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    let response = ui.interact(rect, ui.id().with(("glass-button", label)), if enabled { egui::Sense::click() } else { egui::Sense::hover() });
    let base = if primary { md3::PRIMARY } else { md3::SECONDARY_CONTAINER };
    let accent = if primary { egui::Color32::from_rgb(37, 99, 235) } else { md3::SURFACE_CONTAINER_HIGHEST };
    let fill = if enabled { base } else { md3::SURFACE_CONTAINER_HIGH.linear_multiply(0.55) };
    let text = if enabled { if primary { md3::ON_PRIMARY } else { md3::ON_SECONDARY_CONTAINER } } else { md3::ON_SURFACE_VARIANT };
    let painter = ui.painter();
    painter.rect_filled(rect, 10.0, fill);
    if primary && enabled {
        let right = egui::Rect::from_min_max(egui::pos2(rect.center().x, rect.min.y), rect.max);
        painter.rect_filled(right, 10.0, accent.linear_multiply(0.92));
        painter.rect_stroke(rect.expand(1.5), 11.0, egui::Stroke::new(3.0, md3::PRIMARY.linear_multiply(0.12)), egui::StrokeKind::Outside);
    }
    painter.rect_stroke(rect, 10.0, egui::Stroke::new(1.0, if primary && enabled { md3::PRIMARY.linear_multiply(0.65) } else { md3::OUTLINE_VARIANT }), egui::StrokeKind::Inside);
    painter.text(rect.center(), egui::Align2::CENTER_CENTER, label, egui::FontId::proportional(13.0), text);
    response.clicked()
}

fn gradient_button(ui: &mut egui::Ui, label: &str, enabled: bool, primary: bool) -> bool {
    gradient_button_with_width(ui, label, enabled, primary, ui.available_width().max(112.0))
}

fn m3_button_filled(ui: &mut egui::Ui, label: &str) -> bool {
    gradient_button_with_width(ui, label, true, true, 112.0)
}

fn m3_button_tonal(ui: &mut egui::Ui, label: &str) -> bool {
    gradient_button_with_width(ui, label, true, false, 112.0)
}

fn m3_button_outlined(ui: &mut egui::Ui, label: &str) -> bool {
    gradient_button_with_width(ui, label, true, false, 112.0)
}

fn studio_button(ui: &mut egui::Ui, label: &str, enabled: bool, primary: bool) -> bool {
    gradient_button(ui, label, enabled, primary)
}

fn toggle_switch(ui: &mut egui::Ui, value: &mut bool, label: &str) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(44.0, 24.0), egui::Sense::hover());
        let response = ui.interact(rect, ui.id().with(("toggle", label)), egui::Sense::click());
        if response.clicked() { *value = !*value; changed = true; }
        let track = if *value { md3::PRIMARY } else { md3::SURFACE_CONTAINER_HIGHEST };
        ui.painter().rect_filled(rect, 12.0, track);
        ui.painter().rect_stroke(rect, 12.0, egui::Stroke::new(1.0, if *value { md3::PRIMARY } else { md3::OUTLINE_VARIANT }), egui::StrokeKind::Inside);
        let x = if *value { rect.right() - 12.0 } else { rect.left() + 12.0 };
        ui.painter().circle_filled(egui::pos2(x, rect.center().y), 8.0, if *value { md3::ON_PRIMARY } else { md3::ON_SURFACE_VARIANT });
        ui.add_space(8.0);
        ui.label(egui::RichText::new(label).strong().size(12.0).color(md3::ON_SURFACE));
    });
    changed
}

fn dashed_drop_box<R>(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let frame = egui::Frame::new().fill(md3::SURFACE_CONTAINER.linear_multiply(0.55)).inner_margin(egui::Margin::same(12)).show(ui, |ui| {
        ui.vertical_centered(add_contents).inner
    });
    let rect = frame.response.rect;
    let inner = frame.inner;
    let painter = ui.painter();
    let dash = egui::Stroke::new(1.0, md3::PRIMARY.linear_multiply(0.55));
    let mut x = rect.left();
    while x < rect.right() { painter.line_segment([egui::pos2(x, rect.top()), egui::pos2((x + 6.0).min(rect.right()), rect.top())], dash); painter.line_segment([egui::pos2(x, rect.bottom()), egui::pos2((x + 6.0).min(rect.right()), rect.bottom())], dash); x += 10.0; }
    let mut y = rect.top();
    while y < rect.bottom() { painter.line_segment([egui::pos2(rect.left(), y), egui::pos2(rect.left(), (y + 6.0).min(rect.bottom()))], dash); painter.line_segment([egui::pos2(rect.right(), y), egui::pos2(rect.right(), (y + 6.0).min(rect.bottom()))], dash); y += 10.0; }
    inner
}

fn kpi_card(ui: &mut egui::Ui, value: impl std::fmt::Display, label: &str, color: egui::Color32) {
    glass_card(ui, |ui| {
        ui.label(egui::RichText::new(value.to_string()).size(24.0).strong().color(color));
        ui.label(egui::RichText::new(label).size(10.5).color(md3::ON_SURFACE_VARIANT));
    });
}

fn floating_dock<R>(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::new().fill(md3::SURFACE_CONTAINER.linear_multiply(0.96)).stroke(egui::Stroke::new(1.0, md3::PRIMARY.linear_multiply(0.20))).corner_radius(18).inner_margin(egui::Margin::symmetric(14, 10)).show(ui, |ui| add_contents(ui)).inner
}

fn iphone_mockup_frame<R>(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::new().fill(egui::Color32::from_rgb(5, 8, 14)).stroke(egui::Stroke::new(2.0, md3::OUTLINE_VARIANT)).corner_radius(22).inner_margin(12).show(ui, |ui| add_contents(ui)).inner
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
        // eframe can restore older saved visual preferences after construction.
        if !ctx.style().visuals.dark_mode {setup_custom_theme(ctx);}
        self.brand.refresh_scale(ctx);
        self.tick_cloud(ctx);
        if (!crate::storage::is_demo() || self.auth_preview) && !self.account_authenticated && !self.library_authenticated {
            let logo=self.brand.hero.clone();
            egui::CentralPanel::default().frame(egui::Frame::new().fill(md3::SURFACE).inner_margin(egui::Margin::symmetric(28,16))).show(ctx,|ui| {
                crate::auth_ui::page(ui,&logo,|ui| {
                            if self.auth_preview {ui.label("DEMOSTRACIÓN AISLADA · sin cuentas, correos ni teléfono");ui.horizontal_wrapped(|ui| {
                                if ui.button("Vista correo").clicked() {self.account_stage=0;}
                                if ui.button("Vista contraseña").clicked() {self.account_stage=1;}
                                if ui.button("Vista activación").clicked() {self.account_stage=2;self.account_recovery=false;}
                                if ui.button("Vista recuperación").clicked() {self.account_stage=2;self.account_recovery=true;}
                            });}
                            self.show_account_tab(ui,ctx);
                            ui.add_space(18.0);
                            ui.label(egui::RichText::new("Acceso privado para clientes · hasta 3 equipos por cuenta").size(11.0).color(md3::ON_SURFACE_VARIANT));
                });
            });
            return;
        }
        if !crate::storage::is_demo() && !self.account_authenticated && self.library_authenticated && !self.session_pending {
            egui::CentralPanel::default().show(ctx,|ui| {
                ui.label("Cuenta con pack de diseños · necesitas la licencia de Wallet Studio para utilizar el iPhone y sus copias.");
                ui.horizontal(|ui| {if ui.button("Biblioteca").clicked() {self.current_tab=AppTab::Library;}if ui.button("Cuenta").clicked() {self.current_tab=AppTab::Account;}});
                egui::ScrollArea::vertical().show(ui,|ui| {if self.current_tab==AppTab::Account {self.show_account_tab(ui,ctx);}else{self.show_library_tab(ctx,ui);}});
            });
            return;
        }
        self.handle_messages();
        self.tick_card_cache(ctx);
        self.tick_auto_capture();
        let language = self.language;

        if self.is_busy || self.scanning_syslog {
            ctx.request_repaint_after(std::time::Duration::from_millis(60));
        }

        if self.session_pending {
            egui::TopBottomPanel::top("session-pending-banner").show(ctx,|ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label("Sesión guardada · conexión pendiente. Puedes preparar diseños; el iPhone espera la verificación.");
                    if studio_button(ui,"Comprobar conexión",self.cloud_rx.is_none() && !self.is_busy,false) {self.start_cloud_sync(ctx.clone());}
                });
            });
        }

        // Top bar
        egui::TopBottomPanel::top("header")
            .frame(
                egui::Frame::new()
                    .fill(md3::SURFACE.linear_multiply(0.96))
                    .stroke(egui::Stroke::new(1.0, md3::OUTLINE_VARIANT))
                    .inner_margin(egui::Margin::symmetric(16, 7)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.add(egui::Image::new((self.brand.header.id(), egui::vec2(34.0, 34.0))).corner_radius(10));
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

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui| {
                        if crate::storage::is_demo() {
                            let empty=self.saved_cards.is_empty();
                            if ui.add(egui::Button::new(if empty {"Cargar ejemplos"} else {"Vista vacía"}).min_size(egui::vec2(130.0,44.0))).clicked() {
                                if empty {self.seed_demo(ctx);} else {
                                    self.saved_cards.clear();self.card_hash.clear();self.card_name_draft.clear();self.card_snapshots.invalidate();
                                    self.original_thumbnails.clear();self.applied_thumbnails.clear();self.source_image=None;self.source_texture=None;self.skin=None;
                                    self.gif_frames.clear();self.gif_playback=None;self.crop_draft=None;self.crop_dirty=false;
                                }
                            }
                        }
                        ui.label(egui::RichText::new(if crate::storage::is_demo() {"DEMOSTRACIÓN AISLADA"} else {"ESTUDIO DE TARJETAS"}).size(10.0).color(md3::ON_SURFACE_VARIANT));
                    });
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("DISPOSITIVO / CONEXIÓN").strong().size(10.0).color(md3::ON_SURFACE_VARIANT));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if m3_button_outlined(ui, language.text("Refresh")) {
                            self.refresh_devices();
                        }

                        ui.add_space(6.0);
                        let controls_enabled = !self.is_busy && !self.scanning_syslog && !self.session_pending;
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

        egui::SidePanel::left("studio-navigation").resizable(false).exact_width(148.0)
            .frame(egui::Frame::new().fill(md3::SURFACE.linear_multiply(0.92)).stroke(egui::Stroke::new(1.0, md3::OUTLINE_VARIANT)).inner_margin(egui::Margin::symmetric(12,16))).show(ctx,|ui| {
                 ui.label(egui::RichText::new("ESPACIOS").size(10.0).color(md3::ON_SURFACE_VARIANT));ui.add_space(12.0);
                 for (index,(tab,label)) in [(AppTab::Wallet,"Tarjetas"),(AppTab::Library,"Biblioteca"),(AppTab::Passcode,"Teclado"),(AppTab::Account,"Cuenta"),(AppTab::Help,"Ayuda")].into_iter().enumerate() {
                     let selected=self.current_tab==tab;
                     let (rect,response)=ui.allocate_exact_size(egui::vec2(ui.available_width(),44.0),egui::Sense::click());
                     if selected {ui.painter().rect_filled(rect,10.0,md3::PRIMARY_CONTAINER.linear_multiply(0.72));ui.painter().rect_stroke(rect.expand(1.0),11.0,egui::Stroke::new(2.0,md3::PRIMARY.linear_multiply(0.22)),egui::StrokeKind::Outside);}
                     ui.scope_builder(egui::UiBuilder::new().max_rect(rect.shrink2(egui::vec2(10.0,7.0))),|ui| {ui.horizontal(|ui| {draw_nav_icon(ui,index,selected);ui.add_space(8.0);ui.label(egui::RichText::new(label).strong().color(if selected {md3::PRIMARY} else {md3::ON_SURFACE_VARIANT}));});});
                     if response.clicked() {if tab==AppTab::Library {self.refresh_design_library();} self.current_tab=tab;}
                 }
                 ui.add_space(24.0);ui.separator();ui.add_space(12.0);
                 ui.label(egui::RichText::new("Tus originales").strong().size(12.0));
                 ui.label(egui::RichText::new(if self.cloud_configured {if self.cloud_synced {"Nube verificada"} else {"Pendiente de conexión"}} else {"Guardados en este PC"}).size(10.5).color(md3::ON_SURFACE_VARIANT));
             });
        if self.current_tab==AppTab::Wallet {
            egui::TopBottomPanel::bottom("studio-primary-actions")
                .frame(egui::Frame::new().fill(md3::SURFACE_CONTAINER.linear_multiply(0.96)).stroke(egui::Stroke::new(1.0,md3::PRIMARY.linear_multiply(0.16))).corner_radius(18).inner_margin(egui::Margin::symmetric(20,10)))
                .show(ctx,|ui| { self.show_wallet_primary_actions(ctx,ui); });
        }

        // Central - same SURFACE fill as header/status for flat look
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(md3::SURFACE)
                    .inner_margin(egui::Margin::same(18)),
            )
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    match self.current_tab {
                        AppTab::Wallet => self.show_wallet_tab(ctx, ui),
                        AppTab::Library => self.show_library_tab(ctx, ui),
                        AppTab::Passcode => self.show_passcode_tab(ctx, ui),
                        AppTab::Help => self.show_help_tab(ui),
                        AppTab::Account => self.show_account_tab(ui,ctx),
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
        self.card_name_draft = self.saved_cards.iter()
            .find(|card| card.hash == hash)
            .map(|card| card.name.clone())
            .unwrap_or_default();
        self.rename_error = None;
        let connected = self.selected_udid.as_deref().filter(|selected|
            self.devices.iter().any(|device| device.udid.eq_ignore_ascii_case(selected)));
        let expected = capture_device_for_selection(connected,self.offline_baseline_device.get(hash).map(String::as_str))
            .or_else(|| self.selected_udid.clone());
        self.auto_capture = if crate::storage::is_demo() {None} else {Some(AutoCapture::selected(hash.to_string(),expected))};

        // The normal UI tick starts this request after snapshot safety checks.
        // Syslog detection starts it after retiring the scanner's message batch.
    }

    fn save_current_card_name(&mut self) {
        if crate::storage::is_demo() {
            if let Some(card)=self.saved_cards.iter_mut().find(|card|card.hash==self.card_hash) { card.name=self.card_name_draft.clone(); }
            return;
        }
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

    fn pending_recovery_blocks_selected(&self) -> bool {
        if !self.storage_ready { return true; }
        let pending = if self.recovery_status_error.is_some() { Err(()) } else { Ok(self.pending_recoveries.as_slice()) };
        crate::wallet_export::pending_recovery_blocks_device(pending,self.selected_udid.as_deref())
    }

    fn tick_card_cache(&mut self, ctx: &egui::Context) {
        if crate::storage::is_demo() { return; }
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
        if self.cloud_rx.is_some() || crate::storage::is_demo() || !self.storage_ready { return; }
        let Some(request) = self.auto_capture.as_ref() else { return; };
        if request.hash != self.card_hash { return; }
        let Some(snapshot) = self.snapshot(&request.hash) else { return; };
        let original = snapshot.original;
        let known = snapshot.known_device.clone();
        let safe = !self.is_busy && !self.scanning_syslog && self.selected_usb_ready()
            && !self.pending_recovery_blocks_selected()
            && self.card_cache_rx.is_none() && snapshot.phase.is_ok();
        if self.auto_capture.as_mut().is_some_and(|request|
            request.should_start(self.selected_udid.as_deref(),known.as_deref(),original,safe)) {
            self.capture_original_backup();
        }
    }
    fn show_wallet_primary_actions(&mut self,ctx:&egui::Context,ui:&mut egui::Ui) {
        let selected_hash=self.card_hash.clone();let original=self.original_verified(&selected_hash);
        let version_ok=self.devices.iter().find(|device|Some(&device.udid)==self.selected_udid.as_ref())
            .is_some_and(|device|crate::device::wallet_version_warning(&device.ios_version).is_none());
        let connected=self.selected_transport_available() && version_ok && !self.pending_recovery_blocks_selected() && self.recovery_status_error.is_none() && !crate::storage::is_demo();
        let phase=self.selected_udid.as_ref().map_or(Err(()),|udid|self.bank_intent_phase(udid,&selected_hash));let plan=cached_bank_apply_plan(&phase);
        let idle=!self.is_busy && !self.scanning_syslog && self.cloud_rx.is_none();
        let can_apply=!self.session_pending && connected && idle && original && crate::flasher::BANK_APPLY_READY && self.skin.is_some() && self.gif_playback.is_none() && self.crop_draft.is_none() && !self.crop_dirty && plan!=crate::flasher::BankApplyPlan::Block;
        let can_restore=connected && idle && self.snapshot(&selected_hash).is_some_and(|snapshot|snapshot.restore);
        floating_dock(ui, |ui| {
            ui.columns(3,|buttons| {
                if studio_button(&mut buttons[0],"Elegir imagen…",idle,false) {self.select_skin(ctx);}
                if studio_button(&mut buttons[1],if plan==crate::flasher::BankApplyPlan::RestoreThenApply {"Sustituir diseño"} else {"Aplicar diseño"},can_apply,true) {self.flash_card();}
                if studio_button(&mut buttons[2],"Restaurar original",can_restore,false) {self.restore_original_card();}
            });
            ui.add_space(6.0);
            let message=if crate::storage::is_demo() {"Vista de demostración: no consulta ni modifica un teléfono."}
                else if self.is_busy {self.progress_msg.as_str()}
                else if !self.storage_ready {"Comprobando o recuperando el almacenamiento. Las transferencias esperan."}
                else if self.pending_recovery_blocks_selected() {"Revisa la recuperación pendiente antes de transferir."}
                else if self.cloud_rx.is_some() || (self.cloud_configured && !self.cloud_synced) {self.cloud_status.as_str()}
                else if !original {"Selecciona una tarjeta y espera la verificación de su original. Un diseño personalizado no se certifica como original."}
                else if !connected {"Original verificado en este PC · conecta el iPhone correcto para transferir."}
                else {"Original verificado · mantén el USB conectado y cierra Wallet y Libros durante la transferencia."};
            ui.label(egui::RichText::new(message).size(11.0).color(md3::ON_SURFACE_VARIANT));
            if self.is_busy && self.progress_total>0 {ui.add(egui::ProgressBar::new(self.progress_step as f32/self.progress_total as f32));}
        });
    }
    fn show_wallet_tab(&mut self,ctx:&egui::Context,ui:&mut egui::Ui) {
        let language=self.language;
        let version_warning=self.devices.iter().find(|device|Some(&device.udid)==self.selected_udid.as_ref()).and_then(|device|crate::device::wallet_version_warning(&device.ios_version));
        ui.horizontal(|ui| {
            ui.vertical(|ui| {ui.label(egui::RichText::new("Tu estudio Wallet").strong().size(24.0));ui.label(egui::RichText::new("Elige una tarjeta. Prepara un diseño. Conserva su original.").size(12.0).color(md3::ON_SURFACE_VARIANT));});
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui| {
                let button=egui::Button::new(egui::RichText::new(if self.scanning_syslog {"Detener búsqueda"} else {"Añadir tarjeta"}).strong().size(12.0).color(md3::ON_SECONDARY_CONTAINER))
                    .fill(md3::SECONDARY_CONTAINER).corner_radius(10).min_size(egui::vec2(190.0,44.0));
                if ui.add_enabled((self.scanning_syslog || !self.is_busy) && !crate::storage::is_demo(),button).clicked() {self.toggle_syslog_scan();}
            });
        });
        if self.scanning_syslog {ui.horizontal(|ui| {ui.spinner();ui.label("Abre Wallet en el iPhone y toca la tarjeta que quieres añadir.");});}
        if let Some(warning)=&version_warning {ui.label(egui::RichText::new(warning).color(md3::ERROR));}
        ui.add_space(16.0);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("TUS TARJETAS").strong().size(11.0).color(md3::ON_SURFACE));
            ui.label(egui::RichText::new(format!("{} guardadas",self.saved_cards.len())).size(11.0).color(md3::ON_SURFACE_VARIANT));
        });
        ui.label(egui::RichText::new("Último diseño verificado en este equipo · no es una lectura actual del iPhone").size(10.5).color(md3::ON_SURFACE_VARIANT));
        ui.add_space(10.0);
        if self.saved_cards.is_empty() {
            m3_card(ui,|ui| {ui.label("Aún no tienes tarjetas guardadas. Pulsa Añadir tarjeta con el iPhone conectado para empezar.");});
        } else {
            let tile_width=ui.available_width().min(320.0);
            let cards=self.saved_cards.clone();
            ui.scope(|ui| {
                ui.spacing_mut().item_spacing=egui::vec2(16.0,16.0);
                ui.horizontal_wrapped(|ui| {
                    for card in &cards {
                        let current=self.applied_thumbnail(ctx,&card.hash);let original=self.original_thumbnail(ctx,&card.hash);
                        let verified=self.original_verified(&card.hash);let foreign=self.snapshot(&card.hash).is_some_and(|snapshot|snapshot.reference_device.is_some());
                            if card_gallery::draw_card_tile(ui,tile_width,&card.name,&card.hash,current.as_ref(),original.as_ref(),verified,foreign,self.card_hash==card.hash,language) && !self.is_busy {
                                self.select_saved_card(&card.hash);ctx.request_repaint();
                            }
                    }
                });
            });
        }
        ui.add_space(12.0);
        ui.columns(2,|columns| {
            m3_card(&mut columns[0],|ui| {
                ui.set_min_height(400.0);ui.label(egui::RichText::new("01  TU TARJETA").strong().size(10.0).color(md3::PRIMARY));
                let selected=self.saved_cards.iter().find(|card|card.hash==self.card_hash).cloned();
                let title=selected.as_ref().map(|card|card.name.as_str()).unwrap_or("Selecciona una tarjeta");
                ui.add(egui::Label::new(egui::RichText::new(title).strong().size(19.0)).truncate()).on_hover_text(title);
                ui.add_space(12.0);
                if let Some(card)=selected {
                    let current=self.applied_thumbnail(ctx,&card.hash);let original=self.original_thumbnail(ctx,&card.hash);
                    studio_artwork(ui,current.as_ref().or(original.as_ref()),ui.available_width().min(420.0));
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if let Some(texture)=original.as_ref() {ui.add(egui::Image::new((texture.id(),texture.size_vec2())).fit_to_exact_size(egui::vec2(56.0,36.0)).maintain_aspect_ratio(true).corner_radius(4));}
                        ui.vertical(|ui| {ui.label(egui::RichText::new(if crate::storage::is_demo() {"Original sintético"} else if current.is_some() {"Último diseño aplicado"} else {"Original guardado"}).size(11.0).strong());ui.label(egui::RichText::new(if self.snapshot(&card.hash).is_some_and(|snapshot|snapshot.reference_device.is_some()) {"Referencia local de otro iPhone · no autoriza transferir"} else {"Copia local · no es una lectura actual del iPhone"}).size(10.0).color(md3::ON_SURFACE_VARIANT));});
                    });
                    ui.add_space(10.0);ui.separator();ui.add_space(8.0);
                    ui.label(egui::RichText::new("Nombre de la tarjeta").size(11.0).color(md3::ON_SURFACE_VARIANT));
                    ui.add_enabled(!self.is_busy,egui::TextEdit::singleline(&mut self.card_name_draft).desired_width(ui.available_width()));
                    if studio_button(ui,"Guardar nombre",!self.is_busy,false) {self.save_current_card_name();}
                    if let Some(error)=&self.rename_error {ui.label(egui::RichText::new(error).color(md3::ERROR));}
                    let verified=self.original_verified(&card.hash);ui.label(egui::RichText::new(if verified {"Original verificado en este equipo"} else {"Original pendiente de verificación"}).size(11.0).color(if verified {md3::SUCCESS} else {md3::ON_SURFACE_VARIANT}));
                } else {studio_artwork(ui,None,ui.available_width().min(420.0));ui.add_space(12.0);ui.label("Tu tarjeta aparecerá aquí al seleccionarla. Puedes preparar una imagen mientras conectas el iPhone.");}

            });
            m3_card(&mut columns[1],|ui| {
                ui.set_min_height(400.0);ui.label(egui::RichText::new("02  NUEVO DISEÑO").strong().size(10.0).color(md3::PRIMARY));
                ui.label(egui::RichText::new("Prepara tu próxima tarjeta").strong().size(19.0));ui.add_space(12.0);
                    let width = ui.available_width().min(420.0);
                    let height = width * (969.0 / 1536.0);
                    if let Some(texture) = &self.source_texture {
                        let uv = self.source_image.as_ref().map(|source| {
                            self.current_crop().uv(source.width(), source.height())
                        }).unwrap_or([0.0, 0.0, 1.0, 1.0]);
                        let crop_rect = egui::Rect::from_min_max(egui::pos2(uv[0], uv[1]), egui::pos2(uv[2], uv[3]));
                        let response = ui.add(egui::Image::new((texture.id(), texture.size_vec2())).uv(crop_rect).fit_to_exact_size(egui::vec2(width, height)).maintain_aspect_ratio(false).corner_radius(12).sense(if self.is_busy || self.gif_playback.is_some() || self.crop_draft.is_some() { egui::Sense::hover() } else { egui::Sense::drag() }));
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
                        ui.painter().rect_filled(rect, 12.0, md3::SURFACE_CONTAINER_HIGH);
                        ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, language.text("Imagen no disponible"), egui::FontId::proportional(12.0), md3::ON_SURFACE_VARIANT);
                    }
                    if let Some(skin) = &self.skin {
                        let filename = self.source_path.as_ref().and_then(|path| path.file_name()).and_then(|name| name.to_str()).unwrap_or("Diseño");
                        ui.label(egui::RichText::new(format!("{} · {:.0} KB", filename, skin.png.len() as f32 / 1024.0)).size(10.5).color(md3::PRIMARY));
                    } else {
                        ui.label(egui::RichText::new(language.text("No hay un diseño seleccionado.")).size(11.0).color(md3::ON_SURFACE_VARIANT));
                    }
                ui.add_space(8.0);
                if m3_button_outlined(ui,"Abrir Biblioteca") {self.refresh_design_library();self.current_tab=AppTab::Library;}
                egui::CollapsingHeader::new("Ajustes y exportación").default_open(false).show(ui,|ui| {
                    ui.columns(2,|buttons| {
                        if studio_button(&mut buttons[0],"Recortar imagen…",!self.is_busy && self.gif_playback.is_none() && self.source_image.is_some() && self.crop_draft.is_none(),false) {let crop=self.current_crop();self.crop_draft=Some(if crop.edges.is_some() {crop} else {CardCrop::full_image()});}
                        if studio_button(&mut buttons[1],"Exportar PNG",!self.is_busy && self.gif_playback.is_none() && self.crop_draft.is_none() && !self.crop_dirty && self.skin.is_some(),false) {self.save_prepared_png();}
                    });
                    if studio_button(ui,"Restablecer encuadre",!self.is_busy && self.source_image.is_some() && self.gif_playback.is_none() && self.crop_draft.is_none(),false) {self.crop_focus=[0.5,0.5];self.crop_zoom=1.0;self.crop_edges=None;self.rebuild_skin_from_source();}
                    if studio_button(ui,"Actualizar biblioteca",!self.is_busy,false) {self.refresh_design_library();}
                    ui.label("PNG · JPG · WebP · GIF. Apple Wallet recibe un diseño estático PNG + PDF.");
                    if let Some(error)=&self.library_error {ui.label(egui::RichText::new(error).color(md3::ERROR));}
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

                });
            });
        });
        ui.add_space(10.0);
        egui::CollapsingHeader::new("Verificación, recuperación y detalles técnicos").default_open(false).show(ui,|ui| {
            if ui.add_enabled(!self.is_busy && !crate::storage::is_demo(),egui::TextEdit::singleline(&mut self.card_hash).hint_text("Identificador de tarjeta").desired_width(ui.available_width())).changed() {self.auto_capture=None;self.invalidate_card_cache();}
            let original_verified=self.original_verified(&self.card_hash);
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
                        && ui.add_enabled(!self.is_busy && self.selected_usb_ready() && !self.pending_recovery_blocks_selected()
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
                    if self.pending_recovery_blocks_selected() {
                        let message = if self.selected_udid.is_some() {
                            "Hay una recuperación pendiente de este iPhone. Bloquea todas sus tarjetas hasta resolverla; conserva las copias."
                        } else { "Hay una recuperación pendiente. Selecciona el iPhone correspondiente y conserva las copias." };
                        ui.label(egui::RichText::new(message).color(md3::ERROR));
                    }
                    if self.selected_udid.as_ref().is_some_and(|selected|
                        self.pending_recoveries.iter().any(|entry| !selected.eq_ignore_ascii_case(&entry.udid))) {
                        ui.label("Hay una recuperación pendiente de otro iPhone. Se conserva y no bloquea al seleccionado; conecta aquel dispositivo para revisar su recuperación.");
                    }
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
                    let baseline_pending = self.pending_recoveries.iter().find(|entry|
                        self.selected_udid.as_ref().is_some_and(|udid| udid.eq_ignore_ascii_case(&entry.udid))
                        && entry.original_path.is_none() && entry.baseline_recovery_available).cloned();
                    if let Some(pending) = baseline_pending {
                        ui.label("La exportación no conservó una copia, pero existe el original verificado de esta tarjeta e iPhone. Prepararlo solo guarda evidencia en el PC; la devolución requiere otra acción.");
                        if ui.add_enabled(!self.is_busy && !self.scanning_syslog,
                            egui::Button::new("Preparar recuperación desde mi original verificado")).clicked() {
                            self.prepare_pending_baseline(pending.operation_dir);
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
                let choose_theme=dashed_drop_box(ui, |ui| {
                    ui.add_space(2.0);
                    ui.label(egui::RichText::new(language.text("Theme Package")).size(11.0).color(md3::ON_SURFACE_VARIANT));
                    m3_button_filled(ui, language.text("Choose .passthm..."))
                });
                if choose_theme { self.select_theme_file(ctx); }

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
                let bold_changed=toggle_switch(ui,&mut self.passcode_bold,language.text("Bold Text (iOS Accessibility)"));
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
                .min_size(egui::vec2(ui.available_width(), 44.0));

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

                iphone_mockup_frame(ui, |ui| {
                    ui.vertical_centered(|ui| {
                    if self.keypad_textures.is_empty() {
                        let (rect, _) = ui.allocate_exact_size(egui::vec2(pass_w, pass_h), egui::Sense::hover());
                        let painter = ui.painter();
                        painter.rect_filled(rect, 20.0, egui::Color32::from_rgb(5, 8, 14));
                        painter.rect_stroke(rect, 20.0, egui::Stroke::new(2.0, md3::OUTLINE_VARIANT), egui::StrokeKind::Inside);
                        painter.text(rect.center(), egui::Align2::CENTER_CENTER,
                            language.text("No theme loaded"), egui::FontId::proportional(14.0), md3::ON_SURFACE_VARIANT);
                    } else {
                        let (rect, _) = ui.allocate_exact_size(egui::vec2(pass_w, pass_h), egui::Sense::hover());
                        let painter = ui.painter();
                        painter.rect_filled(rect, 20.0, egui::Color32::from_rgb(5, 8, 14));
                        painter.rect_stroke(rect, 20.0, egui::Stroke::new(2.0, md3::OUTLINE_VARIANT), egui::StrokeKind::Inside);
                        ui.scope_builder(egui::UiBuilder::new().max_rect(rect.shrink(8.0)), |ui| {
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
            ui.label(egui::RichText::new(&self.update_status).color(md3::PRIMARY));
            ui.label(if crate::update_info::PUBLIC_CHANNEL_PUBLISHED {
                "Consulta la versión publicada y descarga la actualización en el navegador."
            } else {
                "El canal público de descargas todavía no está publicado. El estado de la comprobación automática aparece aquí."
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
                ui.label(egui::RichText::new("Selecciona la tarjeta: se comprobará su original antes de aplicar. Una captura nueva exige procedencia exacta del emisor; no se certifica un diseño personalizado.").size(12.0).color(md3::ON_SURFACE_VARIANT));
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
    fn scanner_handoff_retires_old_done_without_losing_capture_result() {
        for done_in_same_batch in [false, true] {
            let (scanner_tx, scanner_rx) = channel();
            let mut receiver = Some(scanner_rx);
            assert!(retire_task_receiver(&mut receiver, done_in_same_batch, true));
            assert!(receiver.is_none());
            let (capture_tx, capture_rx) = channel();
            receiver = Some(capture_rx);
            // A late scanner Done must not be routed to the capture channel.
            assert!(scanner_tx.send("scanner-done").is_err());
            capture_tx.send("original-backup-done").unwrap();
            assert_eq!(receiver.as_ref().unwrap().try_recv().unwrap(), "original-backup-done");
        }
    }
    #[test]
    fn progress_batches_keep_the_active_capture_receiver() {
        let (tx, rx) = channel();
        let mut receiver = Some(rx);
        assert!(!retire_task_receiver(&mut receiver, false, false));
        tx.send("progress").unwrap();
        assert_eq!(receiver.as_ref().unwrap().try_recv().unwrap(), "progress");
        assert!(retire_task_receiver(&mut receiver, true, false));
        assert!(receiver.is_none());
    }
    #[test] fn selected_library_design_is_prepared_before_editor_installation_and_invalid_file_fails() {
        let root=std::env::temp_dir().join(format!("ws-library-use-{}",crate::flasher::generate_token()));std::fs::create_dir_all(&root).unwrap();
        let path=root.join("selected.png");let mut bytes=Vec::new();
        DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(320,200,image::Rgba([14,90,180,255])))
            .write_to(&mut Cursor::new(&mut bytes),image::ImageFormat::Png).unwrap();std::fs::write(&path,&bytes).unwrap();
        let prepared=prepare_library_design(path.clone()).unwrap();assert_eq!(prepared.path,path);
        assert_eq!((prepared.source_image.width(),prepared.source_image.height()),(320,200));
        assert_eq!(prepared.preview.size,[320,200]);assert!(prepared.frames.is_empty());
        let image=crate::design_library::decode_still(&prepared.skin.png).unwrap();assert_eq!((image.width(),image.height()),(1536,969));
        std::fs::write(&path,b"broken download").unwrap();assert!(prepare_library_design(path).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn branded_login_palette_overrides_a_restored_light_theme() {
        let ctx=egui::Context::default();
        ctx.set_theme(egui::ThemePreference::Light);
        assert!(!ctx.style().visuals.dark_mode);
        setup_custom_theme(&ctx);
        assert!(ctx.style().visuals.dark_mode);
        assert_eq!(ctx.style().visuals.panel_fill,md3::SURFACE);
        assert_eq!(ctx.style().visuals.widgets.inactive.fg_stroke.color,md3::ON_SURFACE_VARIANT);
    }
    #[test] fn foreign_reference_art_never_replaces_selected_device_authorization() {
        let mut selected=CardSnapshot {udid:Some("synthetic-B".into()),known_device:Some("synthetic-A".into()),reference_device:None,original:OriginalState::Missing,restore:false,phase:Ok(None),original_image:None,applied_image:None};
        let reference=CardSnapshot {udid:Some("synthetic-A".into()),known_device:Some("synthetic-A".into()),reference_device:None,original:OriginalState::Verified,restore:true,phase:Ok(Some("applied".into())),original_image:Some(egui::ColorImage::from_rgba_unmultiplied([1,1],&[1,2,3,255])),applied_image:None};
        attach_reference(&mut selected,"synthetic-A".into(),reference);
        assert!(selected.original_image.is_some());assert_eq!(selected.reference_device.as_deref(),Some("synthetic-A"));
        assert_eq!(selected.udid.as_deref(),Some("synthetic-B"));assert_eq!(selected.original,OriginalState::Missing);assert!(!selected.restore);assert_eq!(selected.phase,Ok(None));
    }
    #[test] fn foreign_reference_lookup_refuses_multiple_saved_device_identities() {
        let root=std::env::temp_dir().join(format!("ws-reference-fixture-{}",crate::flasher::generate_token()));
        let mut png=Vec::new();DynamicImage::new_rgb8(2,2).write_to(&mut Cursor::new(&mut png),image::ImageFormat::Png).unwrap();
        let sha1=crate::bank_backup::sha1(&png).unwrap();let hash="OM6NYhwXMZrAw0sRUjR62wmF4ZQ=";
        let mut baseline=crate::bank_backup::Baseline {udid:"synthetic-A".into(),card_hash:hash.into(),artwork_name:"cardBackgroundCombined@2x.png".into(),remote_size:png.len(),remote_sha1:sha1.clone(),remote_artwork_bytes:png.clone(),
            manifest_bytes:serde_json::to_vec(&serde_json::json!({"cardBackgroundCombined@2x.png":{"url":"https://a.apple.com/fixture","size":png.len(),"sha1":sha1}})).unwrap(),
            local_assets:crate::wallet_backup::CARD_ARTWORK_ASSETS.iter().map(|name|(name.to_string(),crate::bank_backup::LocalAssetState::Absent)).collect()};
        crate::bank_backup::save_fixture_at(&root,&baseline).unwrap();assert_eq!(unique_reference_device_at(&root,hash).as_deref(),Some("synthetic-A"));
        baseline.udid="synthetic-B".into();crate::bank_backup::save_fixture_at(&root,&baseline).unwrap();assert!(unique_reference_device_at(&root,hash).is_none());std::fs::remove_dir_all(root).unwrap();
    }

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


