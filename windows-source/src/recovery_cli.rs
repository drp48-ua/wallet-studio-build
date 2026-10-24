//! Explicit recovery of one pre-existing interrupted write. No new capture or
//! design is accepted, and no caller-supplied identity/path is ever used.
use anyhow::{Context,Result};
use crate::device::{ConnectionMode,DeviceTransport};

fn exactly_one<T>(mut items:Vec<T>)->Result<T> {
    anyhow::ensure!(items.len()==1,"La recuperación requiere una única coincidencia exacta");
    Ok(items.remove(0))
}
fn candidate()->Result<(String,crate::bank_changes::Intent)> {
    let devices=crate::device::list_connected_devices()?;
    let device=exactly_one(devices.into_iter().filter(|device|device.has_transport(DeviceTransport::Usb)).collect())?;
    let intent=exactly_one(crate::bank_changes::interrupted_restorations_for_device(&device.udid)?)?;
    anyhow::ensure!(intent.udid==device.udid && crate::wallet_export::is_exact_card_hash(&intent.card_hash),"Identidad de recuperación no válida");
    let baseline=crate::bank_backup::load_baseline(&device.udid,&intent.card_hash)?.context("Falta original verificado para la recuperación")?;
    anyhow::ensure!(baseline.udid==device.udid && baseline.card_hash==intent.card_hash,"El original pertenece a otra identidad");
    crate::device::ensure_wallet_version_supported(&device.udid)?;
    let pending=crate::wallet_export::pending_recovery_operations()?;
    anyhow::ensure!(!crate::wallet_export::pending_recovery_blocks_device(Ok(&pending),Some(&device.udid)),"Primero hay que devolver el archivo de una exportación pendiente");
    Ok((device.udid,intent))
}
pub(crate) fn run_if_requested()->bool {
    let args=std::env::args().skip(1).collect::<Vec<_>>();
    let Some(command)=args.first().map(String::as_str) else{return false;};
    if !matches!(command,"--wallet-studio-review-interrupted-write"|"--wallet-studio-restore-interrupted-write"){return false;}
    let result=(||->Result<()> {
        anyhow::ensure!(args.len()==1,"No se aceptan argumentos adicionales");
        let (udid,intent)=candidate()?;
        println!("{}",serde_json::json!({"recovery":"verified_local_original","exact_usb":true,"interrupted_write":true}));
        if command=="--wallet-studio-review-interrupted-write" {return Ok(());}
        crate::flasher::restore_wallet_original(&udid,ConnectionMode::Usb,&intent.card_hash,
            |step,total,_|println!("{}",serde_json::json!({"recovery_step":step,"total":total})),|_|{})?;
        let restored=crate::bank_changes::load_latest_intent(&udid,&intent.card_hash)?.is_some_and(|intent|intent.phase=="restored");
        anyhow::ensure!(restored,"La recuperación sigue pendiente");
        println!("{}",serde_json::json!({"recovery":"original_verified_and_restored"}));Ok(())
    })();
    if result.is_err() {
        // Internal errors can contain identities and paths. This support command
        // emits a fixed category; all original evidence remains in its store.
        eprintln!("RECUPERACION_PENDIENTE: no se confirmó la devolución; se conservan original y diario. Comprueba el iPhone exacto y revisa la recuperación.");
        std::process::exit(2);
    }
    true
}
#[cfg(test)]mod tests {
 use super::*;
 #[test]fn recovery_never_selects_arbitrary_first_device_or_operation(){assert!(exactly_one::<u8>(vec![]).is_err());assert!(exactly_one(vec![1,2]).is_err());assert_eq!(exactly_one(vec![7]).unwrap(),7);}
}
