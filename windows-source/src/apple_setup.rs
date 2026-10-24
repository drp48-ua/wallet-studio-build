//! Official Apple prerequisites. No silent install, elevation, or phone changes.
use anyhow::{Context,Result};
use std::{path::{Path,PathBuf},sync::mpsc::{channel,Receiver}};
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub(crate) enum ServiceState {Missing,Stopped,Running,Unavailable}
#[derive(Clone,Copy,Debug)]
pub(crate) struct Prerequisites {pub runtime:bool,pub service:ServiceState}
impl Prerequisites {pub fn ready(self)->bool {self.runtime && self.service==ServiceState::Running}}
pub(crate) fn probe()->Prerequisites {
    Prerequisites {runtime:crate::apple::verify_support().is_ok(),service:service_status()}
}
#[repr(C)]
struct ServiceStatus {kind:u32,state:u32,controls:u32,exit:u32,specific_exit:u32,checkpoint:u32,hint:u32}
#[link(name="advapi32")]
unsafe extern "system" {
 fn OpenSCManagerW(machine:*const u16,database:*const u16,access:u32)->*mut std::ffi::c_void;
 fn OpenServiceW(manager:*mut std::ffi::c_void,name:*const u16,access:u32)->*mut std::ffi::c_void;
 fn QueryServiceStatus(service:*mut std::ffi::c_void,status:*mut ServiceStatus)->i32;
 fn CloseServiceHandle(handle:*mut std::ffi::c_void)->i32;
}
fn wide(value:&str)->Vec<u16>{value.encode_utf16().chain(Some(0)).collect()}
fn service_status()->ServiceState {
 unsafe {
    let manager=OpenSCManagerW(std::ptr::null(),std::ptr::null(),1);
    if manager.is_null(){return ServiceState::Unavailable;}
    let service=OpenServiceW(manager,wide("Apple Mobile Device Service").as_ptr(),4);
    let error=std::io::Error::last_os_error().raw_os_error();
    CloseServiceHandle(manager);
    if service.is_null(){return if error==Some(1060){ServiceState::Missing}else{ServiceState::Unavailable};}
    let mut status=ServiceStatus{kind:0,state:0,controls:0,exit:0,specific_exit:0,checkpoint:0,hint:0};
    let success=QueryServiceStatus(service,&mut status);CloseServiceHandle(service);
    if success==0 {ServiceState::Unavailable} else if status.state==4 {ServiceState::Running} else {ServiceState::Stopped}
 }
}
#[derive(Debug)]
pub(crate) enum SetupEvent {Downloading,Verifying,Ready(PathBuf),Failed(&'static str)}
pub(crate) fn start_download()->Receiver<SetupEvent> {
    let (tx,rx)=channel();std::thread::spawn(move||{
        let result=(||->Result<PathBuf>{
            let root=crate::storage::root().join("apple-setup");crate::storage::safe_chain(&root)?;std::fs::create_dir_all(&root)?;
            let path=root.join("iTunes64Setup.exe");crate::storage::safe_chain(&path)?;
            if path.exists(){let _=tx.send(SetupEvent::Verifying);verify_installer(&path)?;return Ok(path);}
            let _=tx.send(SetupEvent::Downloading);
            let bytes=crate::auto_update::download_apple_installer()?;
            let staged=root.join(format!("download-{}.exe",crate::flasher::generate_token()));
            crate::storage::write_new(&staged,&bytes)?;
            let _=tx.send(SetupEvent::Verifying);
            if let Err(error)=verify_installer(&staged){let _=std::fs::remove_file(&staged);return Err(error);}
            // Destination is create-only. A concurrent download cannot replace it.
            if std::fs::hard_link(&staged,&path).is_err(){verify_installer(&path)?;}
            let _=std::fs::remove_file(staged);Ok(path)
        })();
        let event=match result {Ok(path)=>SetupEvent::Ready(path),Err(_)=>SetupEvent::Failed("No se pudo descargar o verificar el instalador de Apple. Reintenta o usa la descarga oficial.")};
        let _=tx.send(event);
    });rx
}
fn verify_installer(path:&Path)->Result<()> {
    use base64::Engine;
    crate::storage::safe_chain(path)?;
    let metadata=std::fs::symlink_metadata(path)?;
    anyhow::ensure!(metadata.is_file() && !metadata.file_type().is_symlink() && metadata.len()>0 && metadata.len()<=512*1024*1024,"Instalador fuera de límites");
    let powershell=PathBuf::from(std::env::var_os("SystemRoot").context("Windows no disponible")?).join("System32/WindowsPowerShell/v1.0/powershell.exe");
    // Constant encoded code; the path travels as an environment value, never code.
    // Authenticode validates Windows trust and the leaf publisher must be Apple.
    let script=r#"$ErrorActionPreference='Stop'; try { $s=Get-AuthenticodeSignature -LiteralPath $env:WALLET_STUDIO_APPLE_SETUP_PATH; if ($s.Status -eq 'Valid' -and $s.SignerCertificate.Subject -match '(?:^|,\s*)CN=Apple Inc\.(?:,|$)' -and $s.SignerCertificate.Subject -match '(?:^|,\s*)O=Apple Inc\.(?:,|$)') { exit 0 } else { exit 1 } } catch { exit 1 }"#;
    let bytes=script.encode_utf16().flat_map(u16::to_le_bytes).collect::<Vec<_>>();
    let modules=powershell.parent().context("Módulos de Windows no disponibles")?.join("Modules");
    let mut command=std::process::Command::new(powershell);
    command.args(["-NoLogo","-NoProfile","-NonInteractive","-EncodedCommand",&base64::engine::general_purpose::STANDARD.encode(bytes)])
        .env("WALLET_STUDIO_APPLE_SETUP_PATH",path).env("PSModulePath",modules)
        .stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    use std::os::windows::process::CommandExt;command.creation_flags(0x08000000);
    let mut child=command.spawn()?;let deadline=std::time::Instant::now()+std::time::Duration::from_secs(90);
    loop {
        if let Some(status)=child.try_wait()?{anyhow::ensure!(status.success(),"La firma no acredita un instalador original de Apple");return Ok(());}
        if std::time::Instant::now()>=deadline {let _=child.kill();let _=child.wait();anyhow::bail!("Verificación de firma agotada");}
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}
#[link(name="shell32")]
unsafe extern "system" {fn ShellExecuteW(window:*mut std::ffi::c_void,operation:*const u16,file:*const u16,args:*const u16,directory:*const u16,show:i32)->isize;}
pub(crate) fn launch_verified_installer(path:&Path)->Result<()> {
    verify_installer(path)?;
    let path=path.to_str().context("Ruta del instalador no válida")?;
    let result=unsafe{ShellExecuteW(std::ptr::null_mut(),wide("open").as_ptr(),wide(path).as_ptr(),std::ptr::null(),std::ptr::null(),1)};
    anyhow::ensure!(result>32,"Windows no pudo abrir el instalador de Apple");Ok(())
}
#[cfg(test)]mod tests {
 use super::*;
 #[test]
 #[ignore="explicit read-only signature diagnostic against an already installed Apple executable"]
 fn installed_apple_publisher_signature_is_accepted_without_running_it(){
    assert_eq!(std::env::var("WALLET_STUDIO_APPLE_SIGNATURE_DIAGNOSTIC").as_deref(),Ok("1"));
    let candidates=[Path::new("C:/Program Files/iTunes/iTunes.exe"),Path::new("C:/Program Files/Common Files/Apple/Mobile Device Support/AppleMobileDeviceService.exe")];
    let candidate=candidates.into_iter().find(|path|path.is_file()).expect("No installed Apple executable available for the explicit diagnostic");
    assert!(verify_installer(candidate).is_ok(),"Existing Apple signature was not accepted");
 }
 #[test]fn missing_stopped_or_unknown_service_never_reports_ready(){for service in [ServiceState::Missing,ServiceState::Stopped,ServiceState::Unavailable] {assert!(!Prerequisites{runtime:true,service}.ready());}assert!(!Prerequisites{runtime:false,service:ServiceState::Running}.ready());assert!(Prerequisites{runtime:true,service:ServiceState::Running}.ready());}
 #[test]fn arbitrary_unsigned_executable_cannot_be_offered_as_apple_installer(){let directory=std::env::temp_dir().join(format!("apple-signature-fixture-{}",crate::flasher::generate_token()));std::fs::create_dir(&directory).unwrap();let path=directory.join("fake.exe");std::fs::write(&path,b"MZ synthetic unsigned test data").unwrap();assert!(verify_installer(&path).is_err());std::fs::remove_dir_all(directory).unwrap();}
}
