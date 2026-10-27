//! Pure presentation rules; no state here authorizes a device operation.
use super::ui_card_state::CapturePhase;

pub const CAPTURE_ATTENTION_SECONDS: u64 = 180;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind { Waiting, Running, Attention, Failed, Verified }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action { None, Account, Apple, Usb, Recovery, Retry, Logs }
pub struct Presentation { pub kind: Kind, pub title: String, pub detail: String, pub action: Action }
pub struct Inputs {
    pub phase: Option<CapturePhase>, pub worker_busy: bool, pub elapsed: Option<u64>,
    pub verified: bool, pub storage_ready: bool, pub account_ready: bool,
    pub cloud_ready: bool, pub cloud_running: bool, pub apple_ready: bool,
    pub usb_ready: bool, pub recovery_pending: bool, pub recovery_unknown: bool,
    pub original_blocked: bool, pub foreign_original: bool, pub cache_pending: bool,
}
pub fn present(input: Inputs, phase_label: &str) -> Presentation {
    let state = |kind, title: &str, detail: &str, action| Presentation { kind, title: title.into(), detail: detail.into(), action };
    if input.phase == Some(CapturePhase::Running) {
        if !input.worker_busy || input.elapsed.is_none() {
            return state(Kind::Attention, "Captura sin confirmar", "Comprueba las recuperaciones antes de reintentar. El original aún no está verificado.", Action::Recovery);
        }
        let elapsed = input.elapsed.unwrap_or(0);
        if elapsed >= CAPTURE_ATTENTION_SECONDS {
            return state(Kind::Attention, "La captura necesita atención", &format!("Han pasado {} min {} s. La operación sigue pendiente: mantén el USB y conserva las copias. Consulta Registro; no inicies otra captura.", elapsed / 60, elapsed % 60), Action::Logs);
        }
        return state(Kind::Running, phase_label, &format!("{} min {} s · Mantén USB conectado; Wallet y Libros cerrados. Falta verificar el original y su devolución.", elapsed / 60, elapsed % 60), Action::Logs);
    }
    if !input.storage_ready { return state(Kind::Attention, "Almacenamiento pendiente de revisión", "Abre Cuenta para comprobar tus copias. Las transferencias están bloqueadas.", Action::Account); }
    if input.recovery_unknown && input.cache_pending && !input.verified && !input.recovery_pending && !input.original_blocked {
        return state(Kind::Waiting, "Comprobando recuperaciones", "Espera a la comprobación de tus copias. Todavía no se ha iniciado una captura.", Action::None);
    }
    if input.recovery_pending || input.recovery_unknown || input.original_blocked || input.phase == Some(CapturePhase::Blocked) {
        return state(Kind::Attention, "Recuperación pendiente", "Falta confirmar el estado del original en el iPhone. Comprueba la recuperación pendiente antes de otra captura.", Action::Recovery);
    }
    if input.phase == Some(CapturePhase::Failed) { return state(Kind::Failed, "Captura sin confirmar", "Conserva las copias y comprueba las recuperaciones antes de reintentar.", Action::Retry); }
    if input.foreign_original || input.phase == Some(CapturePhase::ForeignOriginal) {
        return state(Kind::Attention, "Referencia de otro iPhone", "Esta imagen no acredita el original del iPhone seleccionado. Revisa su procedencia; no se iniciará otra captura.", Action::Recovery);
    }
    if input.verified { return state(Kind::Verified, "Original guardado", "Copia original verificada para esta tarjeta e iPhone.", Action::None); }
    if !input.account_ready { return state(Kind::Waiting, "Esperando verificación de acceso", "Abre Cuenta para verificar tu sesión y la licencia de la aplicación.", Action::Account); }
    if !input.cloud_ready || input.cloud_running { return state(Kind::Waiting, "Esperando comprobación de la nube", "No se está guardando un original. Abre Cuenta y revisa el resultado de Sincronizar ahora.", Action::Account); }
    if !input.apple_ready { return state(Kind::Waiting, "Faltan componentes de conexión Apple", "Comprueba los componentes y el servicio de Apple antes de conectar la tarjeta.", Action::Apple); }
    if !input.usb_ready { return state(Kind::Waiting, "Esperando al iPhone por USB", "Conecta el iPhone correcto, desbloquéalo y acepta Confiar. La captura no ha empezado.", Action::Usb); }
    if input.cache_pending { return state(Kind::Waiting, "Comprobando el original", "Verificando las copias locales y el estado de recuperación. Todavía no se ha iniciado una captura.", Action::None); }
    state(Kind::Waiting, "Original pendiente de guardar", "La captura comenzará cuando finalice la operación actual y se comprueben todos los requisitos.", Action::None)
}
pub fn caption(verified: bool, applied: bool, foreign: bool, demo: bool) -> &'static str {
    if demo { "Original sintético" } else if foreign { "Referencia de otro iPhone" }
    else if applied { "Último diseño aplicado" } else if verified { "Original guardado" }
    else { "Original sin verificar" }
}
pub fn capture_phase(line: &str) -> Option<&'static str> {
    let line = line.to_ascii_lowercase();
    if line.contains("restor") || line.contains("return") || line.contains("readback") || line.contains("devol") { Some("Verificando la devolución del original") }
    else if line.contains("download") || line.contains("descarg") { Some("Descargando el original del emisor") }
    else if line.contains("manifest") || line.contains("provenance") || line.contains("procedencia") { Some("Verificando la procedencia del original") }
    else if line.contains("move") || line.contains("remov") || line.contains("retirada") { Some("Preparando la captura recuperable") }
    else { None }
}
/// The caller must enforce isolated demo mode before consulting this preview.
pub fn preview(name: &str, elapsed: u64) -> Option<Presentation> {
    let mut input = Inputs { phase:Some(CapturePhase::Waiting),worker_busy:false,elapsed:None,verified:false,storage_ready:true,account_ready:true,cloud_ready:true,cloud_running:false,apple_ready:true,usb_ready:true,recovery_pending:false,recovery_unknown:false,original_blocked:false,foreign_original:false,cache_pending:false };
    match name {
        "waiting_cloud" => input.cloud_ready=false,
        "waiting_usb" => input.usb_ready=false,
        "apple_missing" => input.apple_ready=false,
        "running" | "attention" => {input.phase=Some(CapturePhase::Running);input.worker_busy=true;input.elapsed=Some(if name=="attention" {elapsed.max(CAPTURE_ATTENTION_SECONDS)} else {elapsed});},
        "failed" => input.phase=Some(CapturePhase::Failed),
        "recovery" => input.recovery_pending=true,
        "verified" => input.verified=true,
        _ => return None,
    }
    Some(present(input,"Descargando el original del emisor"))
}

#[cfg(test)] mod tests {
    use super::*;
    fn ready() -> Inputs { Inputs { phase:Some(CapturePhase::Waiting),worker_busy:false,elapsed:None,verified:false,storage_ready:true,account_ready:true,cloud_ready:true,cloud_running:false,apple_ready:true,usb_ready:true,recovery_pending:false,recovery_unknown:false,original_blocked:false,foreign_original:false,cache_pending:false } }
    #[test] fn prerequisites_never_look_like_an_active_capture() {
        for reason in 0..7 { let mut input=ready(); match reason {0=>input.account_ready=false,1=>input.cloud_ready=false,2=>input.apple_ready=false,3=>input.usb_ready=false,4=>input.cache_pending=true,5=>input.recovery_pending=true,_=>input.storage_ready=false}; let view=present(input,"phase"); assert_ne!(view.kind,Kind::Running);assert_ne!(view.title,"Original guardado");assert!(!view.detail.is_empty()); }
    }
    #[test] fn elapsed_attention_does_not_invent_success_or_finish_the_worker() {
        let mut input=ready();input.phase=Some(CapturePhase::Running);input.worker_busy=true;input.elapsed=Some(179);assert_eq!(present(input,"Comprobando").kind,Kind::Running);
        let mut input=ready();input.phase=Some(CapturePhase::Running);input.worker_busy=true;input.elapsed=Some(180);let view=present(input,"phase");assert_eq!(view.kind,Kind::Attention);assert!(view.detail.contains("sigue pendiente"));assert_eq!(view.action,Action::Logs);
        let mut input=ready();input.phase=Some(CapturePhase::Running);input.verified=true;assert_eq!(present(input,"phase").kind,Kind::Attention);
    }
    #[test] fn image_or_absent_image_cannot_certify_an_unverified_original() {
        for applied in [false,true] {for foreign in [false,true] {assert_ne!(caption(false,applied,foreign,false),"Original guardado");}}
        assert_eq!(caption(true,false,false,false),"Original guardado");
        let mut input=ready();input.phase=Some(CapturePhase::Failed);assert_eq!(present(input,"phase").kind,Kind::Failed);
    }
    #[test] fn verified_pc_copy_does_not_hide_uncertain_phone_return() {
        for unknown in [false,true] {
            let mut input=ready();input.verified=true;input.recovery_unknown=unknown;input.recovery_pending=!unknown;
            let view=present(input,"phase");assert_eq!(view.action,Action::Recovery);assert_eq!(view.kind,Kind::Attention);assert_ne!(view.title,"Original guardado");
        }
        let mut input=ready();input.verified=true;input.phase=Some(CapturePhase::Failed);assert_eq!(present(input,"phase").kind,Kind::Failed);
    }
}
