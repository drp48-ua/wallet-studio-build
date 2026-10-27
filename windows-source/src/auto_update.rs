//! Startup update staging. Application binaries and private state are separate.
use anyhow::{Result, bail};
use std::{path::{Path,PathBuf},time::{Duration,Instant},sync::mpsc::{Receiver,channel}};
use serde::{Serialize,Deserialize};
const REPO:&str="drp48-ua/wallet-studio-build";
const MAX_PACKAGE:usize=128*1024*1024;
fn wide(s:&str)->Vec<u16>{s.encode_utf16().chain(Some(0)).collect()}
#[derive(Debug)]
struct Url {host:String,port:u16,path:String,secure:bool}
fn parse_url(value:&str)->Result<Url>{
    anyhow::ensure!(!value.chars().any(|c|c.is_control())&&!value.contains(['#','\\']),"URL no válida");
    let (secure,tail)=if let Some(v)=value.strip_prefix("https://"){(true,v)}else if let Some(v)=value.strip_prefix("http://"){(false,v)}else{bail!("El servicio requiere HTTPS")};
    let (authority,path)=tail.split_once('/').unwrap_or((tail,""));
    anyhow::ensure!(!authority.contains('@'),"No se permiten credenciales en la URL");
    let (host,port)=if let Some((h,p))=authority.split_once(':'){(h,p.parse::<u16>()?)}else{(authority,if secure{443}else{80})};
    anyhow::ensure!(!host.is_empty()&&host.bytes().all(|c|c.is_ascii_alphanumeric()||b".-".contains(&c))&&port>0,"Servidor no válido");
    anyhow::ensure!(secure||host=="127.0.0.1"||host=="localhost","HTTP solo se permite para pruebas locales");
    Ok(Url{host:host.to_ascii_lowercase(),port,path:format!("/{path}"),secure})
}
#[link(name="winhttp")]
unsafe extern "system" {
 fn WinHttpOpen(a:*const u16,t:u32,p:*const u16,b:*const u16,f:u32)->*mut std::ffi::c_void;
 fn WinHttpConnect(s:*mut std::ffi::c_void,h:*const u16,p:u16,r:u32)->*mut std::ffi::c_void;
 fn WinHttpOpenRequest(c:*mut std::ffi::c_void,m:*const u16,p:*const u16,v:*const u16,r:*const u16,a:*const *const u16,f:u32)->*mut std::ffi::c_void;
 fn WinHttpSetTimeouts(s:*mut std::ffi::c_void,a:i32,b:i32,c:i32,d:i32)->i32;
 fn WinHttpSetOption(s:*mut std::ffi::c_void,o:u32,b:*mut std::ffi::c_void,n:u32)->i32;
 fn WinHttpSendRequest(s:*mut std::ffi::c_void,h:*const u16,n:u32,b:*mut std::ffi::c_void,l:u32,t:u32,c:usize)->i32;
 fn WinHttpReceiveResponse(s:*mut std::ffi::c_void,r:*mut std::ffi::c_void)->i32;
 fn WinHttpQueryHeaders(s:*mut std::ffi::c_void,q:u32,n:*const u16,b:*mut std::ffi::c_void,l:*mut u32,i:*mut u32)->i32;
 fn WinHttpReadData(s:*mut std::ffi::c_void,b:*mut std::ffi::c_void,n:u32,r:*mut u32)->i32;
 fn WinHttpCloseHandle(s:*mut std::ffi::c_void)->i32;
}
struct Handle(*mut std::ffi::c_void);
impl Drop for Handle{fn drop(&mut self){if !self.0.is_null(){unsafe{WinHttpCloseHandle(self.0);}}}}
struct Response{status:u32,body:Vec<u8>,location:Option<String>}
#[derive(Debug)]
pub(crate) struct HttpStatus(pub u32);
impl std::fmt::Display for HttpStatus {
    fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result {write!(f,"El servicio respondió HTTP {}",self.0)}
}
impl std::error::Error for HttpStatus {}
#[derive(Debug)]
pub(crate) struct RequestTimeout;
impl std::fmt::Display for RequestTimeout {
    fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result {f.write_str("El servicio agotó el tiempo de espera")}
}
impl std::error::Error for RequestTimeout {}
fn network_result(ok:i32,message:&str)->Result<()> {
    if ok!=0 {return Ok(());}
    if std::io::Error::last_os_error().raw_os_error()==Some(12002) {return Err(RequestTimeout.into());}
    bail!("{message}")
}
const REQUEST_BUDGET:Duration=Duration::from_secs(60);
const BUNDLE_DOWNLOAD_BUDGET:Duration=Duration::from_secs(300);
fn request(method:&str,value:&str,headers:&[(String,String)],body:&[u8],max:usize,budget:Duration)->Result<Response>{
    request_with_cap(method,value,headers,body,max,budget,MAX_PACKAGE)
}
fn request_with_cap(method:&str,value:&str,headers:&[(String,String)],body:&[u8],max:usize,budget:Duration,cap:usize)->Result<Response>{
    request_until(method,value,headers,body,max,Instant::now()+budget,cap)
}
fn request_until(method:&str,value:&str,headers:&[(String,String)],body:&[u8],max:usize,deadline:Instant,cap:usize)->Result<Response>{
    anyhow::ensure!(Instant::now()<deadline,RequestTimeout);
    let url=parse_url(value)?;
    anyhow::ensure!(matches!(method,"GET"|"POST"|"PUT"|"DELETE")&&max<=cap&&body.len()<=MAX_PACKAGE,"Petición fuera de límites");
    let mut header=String::new();
    for (k,v) in headers {anyhow::ensure!(!k.is_empty()&&k.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'-')&&!v.chars().any(|c|c.is_control()),"Cabecera no válida");header.push_str(&format!("{k}: {v}\r\n"));}
    let agent=wide("Wallet Studio/0.6");let host=wide(&url.host);let path=wide(&url.path);let verb=wide(method);let hw=wide(&header);
    let session=Handle(unsafe{WinHttpOpen(agent.as_ptr(),0,std::ptr::null(),std::ptr::null(),0)});
    anyhow::ensure!(!session.0.is_null(),"No se pudo iniciar la conexión");
    let connection=Handle(unsafe{WinHttpConnect(session.0,host.as_ptr(),url.port,0)});
    anyhow::ensure!(!connection.0.is_null(),"No se pudo conectar con el servicio");
    let req=Handle(unsafe{WinHttpOpenRequest(connection.0,verb.as_ptr(),path.as_ptr(),std::ptr::null(),std::ptr::null(),std::ptr::null(),if url.secure{0x00800000}else{0})});
    anyhow::ensure!(!req.0.is_null(),"No se pudo crear la petición");
    let mut disable=2u32; // WINHTTP_DISABLE_REDIRECTS (SDK); 8 disables keep-alive.
    anyhow::ensure!(unsafe{WinHttpSetOption(req.0,63,(&mut disable as *mut u32).cast(),4)}!=0,"No se pudo bloquear la redirección");
    // Free hosting can cold-start while receiving the first response. Keep the
    // bounded total deadline and short connect timeouts, but allow that wake-up
    // within the remaining receive budget. Calls run off the UI thread.
    let timeout=|h|->Result<()>{let ms=deadline.saturating_duration_since(Instant::now()).as_millis();if ms==0 {return Err(RequestTimeout.into());}let t=ms.min(15000) as i32;let receive=ms.min(60000) as i32;anyhow::ensure!(unsafe{WinHttpSetTimeouts(h,t,t,t,receive)}!=0,"No se pudo configurar el tiempo de espera");Ok(())};
    timeout(req.0)?;
    network_result(unsafe{WinHttpSendRequest(req.0,hw.as_ptr(),header.encode_utf16().count() as u32,if body.is_empty(){std::ptr::null_mut()}else{body.as_ptr() as *mut _},body.len() as u32,body.len() as u32,0)},"No se pudo enviar la petición")?;
    timeout(req.0)?;
    network_result(unsafe{WinHttpReceiveResponse(req.0,std::ptr::null_mut())},"No se recibió respuesta")?;
    let mut status=0u32;let mut len=4;
    anyhow::ensure!(unsafe{WinHttpQueryHeaders(req.0,19|0x20000000,std::ptr::null(),(&mut status as *mut u32).cast(),&mut len,std::ptr::null_mut())}!=0,"Estado HTTP no disponible");
    let location=if matches!(status,301|302|303|307|308){let mut buf=[0u16;8192];let mut n=(buf.len()*2)as u32;let ok=unsafe{WinHttpQueryHeaders(req.0,33,std::ptr::null(),buf.as_mut_ptr().cast(),&mut n,std::ptr::null_mut())};if ok!=0{Some(String::from_utf16(&buf[..buf.iter().position(|x|*x==0).unwrap_or(buf.len())])?)}else{None}}else{None};
    let mut out=Vec::new();let mut buffer=[0u8;8192];
    loop{timeout(req.0)?;let mut n=0;network_result(unsafe{WinHttpReadData(req.0,buffer.as_mut_ptr().cast(),8192,&mut n)},"Lectura de red incompleta")?;if n==0{break;}anyhow::ensure!(out.len().saturating_add(n as usize)<=max,"Respuesta demasiado grande");out.extend_from_slice(&buffer[..n as usize]);}
    if Instant::now()>=deadline {return Err(RequestTimeout.into());}
    Ok(Response{status,body:out,location})
}
const MAX_GET_ATTEMPTS:usize=8;
trait RetryClock {
    fn now(&self)->Instant;
    fn sleep(&mut self,delay:Duration);
}
struct WallClock;
impl RetryClock for WallClock {
    fn now(&self)->Instant {Instant::now()}
    fn sleep(&mut self,delay:Duration) {std::thread::sleep(delay);}
}
// Only completed transient HTTP responses are retryable. Transport failures,
// oversized/malformed reads and authentication failures retain their errors.
fn retry_get_response(method:&str,deadline:Instant,clock:&mut dyn RetryClock,
    attempt:&mut dyn FnMut(Instant)->Result<Response>)->Result<Response> {
    for index in 0..MAX_GET_ATTEMPTS {
        anyhow::ensure!(clock.now()<deadline,RequestTimeout);
        let response=attempt(deadline)?;
        anyhow::ensure!(clock.now()<deadline,RequestTimeout);
        if method!="GET" || !matches!(response.status,502|503|504) || index+1==MAX_GET_ATTEMPTS {return Ok(response);}
        let delay=Duration::from_millis((500u64<<index.min(4)).min(8000));
        // Do not wait when the entire remaining budget would be consumed by
        // backoff; a new request must never get a fresh caller deadline.
        anyhow::ensure!(deadline.saturating_duration_since(clock.now())>delay,RequestTimeout);
        clock.sleep(delay);
    }
    unreachable!("bounded retry loop always returns")
}
fn request_with_get_retry(method:&str,url:&str,headers:&[(String,String)],body:&[u8],max:usize,deadline:Instant)->Result<Response> {
    retry_get_response(method,deadline,&mut WallClock,&mut |deadline|request_until(method,url,headers,body,max,deadline,MAX_PACKAGE))
}
pub fn http_request(method:&str,url:&str,headers:&[(String,String)],body:&[u8],max_bytes:usize)->Result<Vec<u8>>{
    http_request_with_budget(method,url,headers,body,max_bytes,REQUEST_BUDGET)
}
pub(crate) fn http_request_with_budget(method:&str,url:&str,headers:&[(String,String)],body:&[u8],max_bytes:usize,remaining:Duration)->Result<Vec<u8>>{
    anyhow::ensure!(!remaining.is_zero(),RequestTimeout);
    let deadline=Instant::now()+remaining.min(REQUEST_BUDGET);
    let r=request_with_get_retry(method,url,headers,body,max_bytes,deadline)?;
    anyhow::ensure!(matches!(r.status,200|201|204),HttpStatus(r.status));
    Ok(r.body)
}
/// Large immutable originals need a bounded transfer budget independent of
/// account/catalog requests. Redirects, response caps and caller checks remain.
pub(crate) fn http_download_bundle(url:&str,headers:&[(String,String)],max_bytes:usize)->Result<Vec<u8>> {
    http_download_bundle_with_budget(url,headers,max_bytes,BUNDLE_DOWNLOAD_BUDGET)
}
pub(crate) fn http_download_bundle_with_budget(url:&str,headers:&[(String,String)],max_bytes:usize,remaining:Duration)->Result<Vec<u8>> {
    anyhow::ensure!(!remaining.is_zero(),RequestTimeout);
    let deadline=Instant::now()+remaining.min(BUNDLE_DOWNLOAD_BUDGET);
    let parsed=parse_url(url)?;
    let id=parsed.path.rsplit('/').next().unwrap_or_default();
    anyhow::ensure!(parsed.path.contains("/v1/bundles/") && id.len()==64 && id.bytes().all(|b|b.is_ascii_hexdigit()) && max_bytes<=100*1024*1024,"Descarga de copia fuera de límites");
    let response=request_with_get_retry("GET",url,headers,&[],max_bytes,deadline)?;
    anyhow::ensure!(response.status==200,HttpStatus(response.status));
    Ok(response.body)
}
fn public_download(url:&str,max:usize)->Result<Vec<u8>>{
    let mut next=url.to_owned();
    for _ in 0..4{let p=parse_url(&next)?;anyhow::ensure!(p.secure&&matches!(p.host.as_str(),"github.com"|"release-assets.githubusercontent.com"|"objects.githubusercontent.com"),"Origen de actualización no permitido");let r=request("GET",&next,&[],&[],max,REQUEST_BUDGET)?;if r.status==200{return Ok(r.body);}anyhow::ensure!(matches!(r.status,301|302|303|307|308),"Descarga respondió HTTP {}",r.status);next=r.location.ok_or_else(||anyhow::anyhow!("Redirección incompleta"))?;}
    bail!("Demasiadas redirecciones de actualización")
}
fn apple_download_url(url:&str)->bool {
    parse_url(url).is_ok_and(|url|url.secure && url.port==443 &&
        (url.host=="apple.com" || url.host.ends_with(".apple.com") || url.host=="cdn-apple.com" || url.host.ends_with(".cdn-apple.com")))
}
pub(crate) fn download_apple_installer()->Result<Vec<u8>> {
    let mut next="https://www.apple.com/itunes/download/win64/".to_owned();
    let deadline=Instant::now()+Duration::from_secs(900);
    for _ in 0..4 {
        anyhow::ensure!(apple_download_url(&next),"Origen del instalador Apple no permitido");
        let remaining=deadline.saturating_duration_since(Instant::now());
        anyhow::ensure!(!remaining.is_zero(),RequestTimeout);
        let response=request_with_cap("GET",&next,&[],&[],512*1024*1024,remaining,512*1024*1024)?;
        if response.status==200 {anyhow::ensure!(response.body.starts_with(b"MZ"),"Instalador Apple no válido");return Ok(response.body);}
        anyhow::ensure!(matches!(response.status,301|302|303|307|308),HttpStatus(response.status));
        next=response.location.ok_or_else(||anyhow::anyhow!("Redirección del instalador incompleta"))?;
    }
    bail!("Demasiadas redirecciones del instalador Apple")
}
#[link(name="bcrypt")]
unsafe extern "system"{fn BCryptOpenAlgorithmProvider(h:*mut *mut std::ffi::c_void,n:*const u16,i:*const u16,f:u32)->i32;fn BCryptHash(h:*mut std::ffi::c_void,k:*const u8,kn:u32,b:*const u8,bn:u32,o:*mut u8,on:u32)->i32;fn BCryptCloseAlgorithmProvider(h:*mut std::ffi::c_void,f:u32)->i32;
fn BCryptImportKeyPair(a:*mut std::ffi::c_void,k:*mut std::ffi::c_void,t:*const u16,o:*mut *mut std::ffi::c_void,b:*const u8,n:u32,f:u32)->i32;
fn BCryptVerifySignature(k:*mut std::ffi::c_void,p:*const std::ffi::c_void,h:*const u8,n:u32,s:*const u8,l:u32,f:u32)->i32;
fn BCryptDestroyKey(k:*mut std::ffi::c_void)->i32;}
pub(crate) fn sha256(bytes:&[u8])->Result<String>{let mut h=std::ptr::null_mut();let alg=wide("SHA256");let mut out=[0;32];unsafe{anyhow::ensure!(BCryptOpenAlgorithmProvider(&mut h,alg.as_ptr(),std::ptr::null(),0)==0,"SHA-256 no disponible");let status=BCryptHash(h,std::ptr::null(),0,bytes.as_ptr(),u32::try_from(bytes.len())?,out.as_mut_ptr(),32);BCryptCloseAlgorithmProvider(h,0);anyhow::ensure!(status==0,"No se pudo verificar SHA-256");}Ok(out.iter().map(|b|format!("{b:02x}")).collect())}
#[derive(Serialize,Deserialize,Clone)]
struct Manifest{version:String,build_id:u64,repository:String,source_branch:String,commit:String,asset:String,sha256:String,size:usize,exe_sha256:String}
#[derive(Serialize,Deserialize,Clone)]
struct SignedManifest { manifest_bytes:Vec<u8>, signature:Vec<u8> }
impl SignedManifest {
    fn read(&self)->Result<Manifest>{
        anyhow::ensure!(self.manifest_bytes.len()<=65536&&self.signature.len()==384,"Firma de publicación fuera de límites");
        verify_signature(&self.manifest_bytes,&self.signature)?;
        let manifest:Manifest=serde_json::from_slice(&self.manifest_bytes)?;
        validate_manifest(&manifest)?;Ok(manifest)
    }
}
fn verify_signature(bytes:&[u8],signature:&[u8])->Result<()> {
    anyhow::ensure!(bytes.len()<=65536&&signature.len()==384,"Firma de publicación no válida");
    let hex=sha256(bytes)?;
    let digest=(0..32).map(|i|u8::from_str_radix(&hex[i*2..i*2+2],16)).collect::<std::result::Result<Vec<_>,_>>()?;
    #[repr(C)] struct Pss{algorithm:*const u16,salt:u32}
    let algorithm=wide("RSA");let blob_type=wide("RSAPUBLICBLOB");let sha=wide("SHA256");
    let public=include_bytes!("../assets/update-public-key.blob");
    let mut provider=std::ptr::null_mut();let mut key=std::ptr::null_mut();
    unsafe {
        anyhow::ensure!(BCryptOpenAlgorithmProvider(&mut provider,algorithm.as_ptr(),std::ptr::null(),0)==0,"Verificación de firma no disponible");
        let imported=BCryptImportKeyPair(provider,std::ptr::null_mut(),blob_type.as_ptr(),&mut key,public.as_ptr(),public.len()as u32,0);
        if imported!=0{BCryptCloseAlgorithmProvider(provider,0);bail!("Clave pública de actualización no válida")}
        let padding=Pss{algorithm:sha.as_ptr(),salt:32};
        let result=BCryptVerifySignature(key,(&padding as *const Pss).cast(),digest.as_ptr(),32,signature.as_ptr(),signature.len()as u32,8);
        BCryptDestroyKey(key);BCryptCloseAlgorithmProvider(provider,0);
        anyhow::ensure!(result==0,"La firma de actualización no pertenece al editor autorizado");
    }Ok(())
}
fn version(value:&str)->Result<[u64;3]>{let p=value.split('.').map(str::parse::<u64>).collect::<std::result::Result<Vec<_>,_>>()?;anyhow::ensure!(p.len()==3,"Versión no válida");Ok([p[0],p[1],p[2]])}
fn validate_manifest(m:&Manifest)->Result<()>{anyhow::ensure!(m.repository==REPO&&m.source_branch=="main"&&m.build_id>0&&m.commit.len()==40&&m.commit.bytes().all(|c|c.is_ascii_hexdigit()),"Publicación fuera del canal autorizado");version(&m.version)?;anyhow::ensure!(m.asset==format!("Wallet-Studio-{}-Windows.zip",m.version)&&m.size>0&&m.size<=MAX_PACKAGE&&[&m.sha256,&m.exe_sha256].iter().all(|h|h.len()==64&&h.bytes().all(|c|c.is_ascii_hexdigit())),"Metadatos de actualización no válidos");Ok(())}
fn cache()->PathBuf{crate::storage::root().join("updates")}
fn active_at(root:&Path)->Result<Option<Manifest>>{let p=root.join("active.json");crate::storage::safe_chain(&p)?;if !p.exists(){return Ok(None);}let meta=std::fs::symlink_metadata(&p)?;anyhow::ensure!(meta.is_file()&&meta.len()<=512*1024,"Registro de actualización no válido");let signed:SignedManifest=serde_json::from_slice(&std::fs::read(p)?)?;Ok(Some(signed.read()?))}
fn active()->Result<Option<Manifest>>{active_at(&cache())}
pub fn redirect_if_ready()->Result<bool>{
    let Some(m)=active()? else{return Ok(false)};
    if version(&m.version)?<version(crate::update_info::VERSION)?{return Ok(false)}
    let target=cache().join(format!("build-{}",m.build_id)).join("Wallet Studio.exe");crate::storage::safe_chain(&target)?;
    let metadata=std::fs::symlink_metadata(&target)?;
    anyhow::ensure!(metadata.is_file()&&!metadata.file_type().is_symlink()&&metadata.len()<=MAX_PACKAGE as u64,"Actualización preparada no disponible");
    let hash=sha256(&std::fs::read(&target)?)?;anyhow::ensure!(hash==m.exe_sha256,"El ejecutable actualizado no supera la verificación");
    if sha256(&std::fs::read(std::env::current_exe()?)?)?==hash{return Ok(false)}
    std::process::Command::new(target).spawn()?;Ok(true)
}
struct UpdateLock(PathBuf);
impl Drop for UpdateLock { fn drop(&mut self){let _=std::fs::remove_file(&self.0);} }
fn require_monotonic(previous:&Manifest,next:&Manifest)->Result<()> {
    let previous_version=version(&previous.version)?;
    let next_version=version(&next.version)?;
    anyhow::ensure!(next_version>previous_version || (next_version==previous_version && next.build_id>=previous.build_id),"El canal intentó retroceder una actualización");
    if next_version==previous_version && next.build_id==previous.build_id { anyhow::ensure!(next.commit==previous.commit&&next.sha256==previous.sha256&&next.exe_sha256==previous.exe_sha256&&next.version==previous.version,"Una publicación inmutable cambió de contenido"); }
    Ok(())
}
fn stage_at(root:&Path,signed:&SignedManifest,bytes:&[u8])->Result<()>{
    use std::io::Write;
    let m=signed.read()?;
    validate_manifest(&m)?;anyhow::ensure!(bytes.len()==m.size&&sha256(bytes)?==m.sha256,"El paquete no coincide con la publicación");
    let mut zip=zip::ZipArchive::new(std::io::Cursor::new(bytes))?;anyhow::ensure!(zip.len()<=2048,"Demasiados archivos en la actualización");
    let files=unpack(&mut zip,&m.version)?;
    let exe=files.iter().find(|(p,_)|p==Path::new("Wallet Studio.exe")).ok_or_else(||anyhow::anyhow!("No hay ejecutable en el paquete"))?;
    anyhow::ensure!(exe.1.starts_with(b"MZ")&&sha256(&exe.1)?==m.exe_sha256,"Ejecutable de actualización no válido");
    crate::storage::safe_chain(root)?;std::fs::create_dir_all(root)?;
    let lock=root.join("staging.lock");crate::storage::safe_chain(&lock)?;
    let mut lock_file=std::fs::OpenOptions::new().write(true).create_new(true).open(&lock).map_err(|_|anyhow::anyhow!("Otra instancia prepara una actualización; si hubo un cierre inesperado conserva el registro para soporte"))?;
    let _lock=UpdateLock(lock);lock_file.write_all(b"Wallet Studio update staging\n")?;lock_file.sync_all()?;drop(lock_file);
    if let Some(previous)=active_at(root)? {require_monotonic(&previous,&m)?;}
    let destination=root.join(format!("build-{}",m.build_id));crate::storage::safe_chain(&destination)?;
    if !destination.exists(){let temp=root.join(format!("staging-{}-{}",m.build_id,crate::flasher::generate_token()));std::fs::create_dir(&temp)?;for(p,b)in &files{let target=temp.join(p);std::fs::create_dir_all(target.parent().unwrap())?;let mut f=std::fs::OpenOptions::new().write(true).create_new(true).open(&target)?;f.write_all(b)?;f.sync_all()?;anyhow::ensure!(std::fs::read(target)?==*b,"Actualización no supera lectura posterior");}anyhow::ensure!(temp.parent()==Some(root)&&destination.parent()==Some(root),"Destino de actualización no válido");std::fs::rename(temp,&destination)?;}
    for (p,b) in &files { let target=destination.join(p);crate::storage::safe_chain(&target)?;let metadata=std::fs::symlink_metadata(&target)?;anyhow::ensure!(metadata.is_file()&&metadata.len()==b.len() as u64&&std::fs::read(target)?==*b,"Instalación preparada no coincide"); }
    let temp=root.join(format!("active-{}.tmp",crate::flasher::generate_token()));let data=serde_json::to_vec(signed)?;let mut f=std::fs::OpenOptions::new().write(true).create_new(true).open(&temp)?;f.write_all(&data)?;f.sync_all()?;drop(f);
    // MoveFileEx is atomic and only replaces our update pointer, never backups.
    #[link(name="kernel32")]unsafe extern "system"{fn MoveFileExW(a:*const u16,b:*const u16,f:u32)->i32;}
    let a=crate::storage::wide_path(&temp)?;let b=crate::storage::wide_path(&root.join("active.json"))?;anyhow::ensure!(unsafe{MoveFileExW(a.as_ptr(),b.as_ptr(),1|8)}!=0,"No se pudo activar la actualización preparada");Ok(())
}
fn unpack<R:std::io::Read+std::io::Seek>(zip:&mut zip::ZipArchive<R>,version:&str)->Result<Vec<(PathBuf,Vec<u8>)>>{
    use std::io::Read;
    anyhow::ensure!(zip.len()<=2048,"Demasiados archivos en la actualización");
    let prefix=format!("Wallet-Studio-{version}/");let mut files=Vec::new();let mut total=0u64;let mut names=std::collections::HashSet::new();
    for i in 0..zip.len(){let mut f=zip.by_index(i)?;if f.is_dir(){continue;}let raw=f.name();let relative=raw.strip_prefix(&prefix).ok_or_else(||anyhow::anyhow!("Raíz de paquete inesperada"))?;let path=Path::new(relative);anyhow::ensure!(!relative.contains(['\\',':'])&&!relative.is_empty()&&path.components().all(|c|matches!(c,std::path::Component::Normal(_)))&&names.insert(relative.to_ascii_lowercase()),"Ruta de paquete no válida");
    let lower=relative.to_ascii_lowercase();let lower_path=Path::new(&lower);
    anyhow::ensure!(!lower_path.components().any(|c|matches!(c.as_os_str().to_str(),Some("bank-backups"|"bank-change-intents"|"wallet-export-recovery"|"quarantine")))&&!matches!(lower_path.file_name().and_then(|n|n.to_str()),Some("cards.json"|"baseline.json"|"verified-original.json"|"manifest.urls"|"remote-artwork.bin"|"cloud-session.dpapi"|"vault-owner.json"))&&f.unix_mode().is_none_or(|m|m&0o170000!=0o120000),"El paquete contiene datos privados o enlaces");
    let size=f.size();total=total.checked_add(size).ok_or_else(||anyhow::anyhow!("Paquete fuera de límites"))?;anyhow::ensure!(total<=256*1024*1024&&size<=MAX_PACKAGE as u64,"Actualización descomprimida demasiado grande");let name=path.to_owned();let mut data=Vec::new();f.by_ref().take(size+1).read_to_end(&mut data)?;anyhow::ensure!(data.len()as u64==size,"Archivo descomprimido incompleto");files.push((name,data));}Ok(files)
}
fn check()->Result<String>{
    let url=format!("https://api.github.com/repos/{REPO}/releases/latest");let data=http_request("GET",&url,&[("Accept".into(),"application/vnd.github+json".into())],&[],1024*1024)?;let release:serde_json::Value=serde_json::from_slice(&data)?;
    anyhow::ensure!(release["draft"]==false&&release["prerelease"]==false,"Publicación no estable");let assets=release["assets"].as_array().ok_or_else(||anyhow::anyhow!("Canal sin paquetes"))?;
    let asset_url=|name:&str|->Result<String>{let matches=assets.iter().filter(|a|a["name"].as_str()==Some(name)).collect::<Vec<_>>();anyhow::ensure!(matches.len()==1,"Paquete de publicación ambiguo");let url=matches[0]["browser_download_url"].as_str().ok_or_else(||anyhow::anyhow!("Descarga no disponible"))?;anyhow::ensure!(url.starts_with(&format!("https://github.com/{REPO}/releases/download/")),"Descarga fuera del repositorio");Ok(url.into())};
    let signed=SignedManifest{manifest_bytes:public_download(&asset_url("release-manifest.json")?,65536)?,signature:public_download(&asset_url("release-manifest.sig")?,384)?};let m=signed.read()?;
    let current=version(crate::update_info::VERSION)?;let v=version(&m.version)?;if v<current{return Ok("La versión instalada es más reciente que el canal publicado".into())}
    let previous=active()?;if previous.is_some_and(|p|p.build_id>=m.build_id&&version(&p.version).is_ok_and(|x|x>=v)){return Ok("Wallet Studio está actualizado".into())}
    if v==current&&option_env!("WALLET_STUDIO_BUILD_ID").and_then(|s|s.parse::<u64>().ok()).is_some_and(|b|b>=m.build_id){return Ok("Wallet Studio está actualizado".into())}
    let package=public_download(&asset_url(&m.asset)?,MAX_PACKAGE)?;stage_at(&cache(),&signed,&package)?;Ok(format!("Actualización {} preparada: se activará al volver a abrir",m.version))
}
pub fn start_background_check()->Receiver<String>{let(tx,rx)=channel();std::thread::spawn(move||{let _=tx.send("Comprobando actualizaciones…".into());let status=match check(){Ok(s)=>s,Err(_)=>"Canal de actualización no disponible; puedes seguir trabajando".into()};let _=tx.send(status);});rx}
pub fn poll_status(rx:&Receiver<String>,status:&mut String)->bool {
    loop {
        match rx.try_recv() {
            Ok(message)=>*status=message,
            Err(std::sync::mpsc::TryRecvError::Empty)=>return false,
            Err(std::sync::mpsc::TryRecvError::Disconnected)=>return true,
        }
    }
}
#[cfg(test)]mod tests{use super::*;
 struct SimulatedClock {start:Instant,elapsed:std::rc::Rc<std::cell::Cell<Duration>>,sleeps:Vec<Duration>,oversleep:Duration}
 impl RetryClock for SimulatedClock {
    fn now(&self)->Instant {self.start+self.elapsed.get()}
    fn sleep(&mut self,delay:Duration) {self.sleeps.push(delay);self.elapsed.set(self.elapsed.get()+delay+self.oversleep);}
 }
 fn simulated_clock()->SimulatedClock {SimulatedClock {start:Instant::now(),elapsed:Default::default(),sleeps:Vec::new(),oversleep:Duration::ZERO}}
 fn synthetic_response(status:u32)->Response {Response {status,body:b"ok".to_vec(),location:None}}
 #[test]fn get_retry_uses_one_deadline_and_stops_after_eight_transient_responses() {
    let mut clock=simulated_clock();let deadline=clock.start+Duration::from_secs(60);let mut calls=0;
    let response=retry_get_response("GET",deadline,&mut clock,&mut |seen| {
        assert_eq!(seen,deadline);calls+=1;Ok(synthetic_response([502,503,504][(calls-1)%3]))
    }).unwrap();
    assert_eq!(calls,8);assert_eq!(response.status,503);
    assert_eq!(clock.sleeps,vec![500,1000,2000,4000,8000,8000,8000].into_iter().map(Duration::from_millis).collect::<Vec<_>>());
 }
 #[test]fn get_retry_deadline_covers_network_and_sleep_without_reset_or_extra_request() {
    // First response consumed 600ms: a 500ms backoff cannot fit in 1s.
    let mut clock=simulated_clock();let elapsed=clock.elapsed.clone();let deadline=clock.start+Duration::from_secs(1);let mut calls=0;
    let error=retry_get_response("GET",deadline,&mut clock,&mut |seen| {
        assert_eq!(seen,deadline);calls+=1;elapsed.set(elapsed.get()+Duration::from_millis(600));Ok(synthetic_response(503))
    }).err().unwrap();
    assert!(error.downcast_ref::<RequestTimeout>().is_some());assert_eq!(calls,1);assert!(clock.sleeps.is_empty());
    // Scheduler oversleep must be checked before the second network attempt.
    let mut clock=simulated_clock();clock.oversleep=Duration::from_secs(1);let deadline=clock.start+Duration::from_secs(1);let mut calls=0;
    let error=retry_get_response("GET",deadline,&mut clock,&mut |_|{calls+=1;Ok(synthetic_response(503))}).err().unwrap();
    assert!(error.downcast_ref::<RequestTimeout>().is_some());assert_eq!(calls,1);assert_eq!(clock.sleeps.len(),1);
    // A response that itself reaches the deadline is never accepted/retried.
    let mut clock=simulated_clock();let elapsed=clock.elapsed.clone();let deadline=clock.start+Duration::from_secs(1);
    let error=retry_get_response("GET",deadline,&mut clock,&mut |_| {elapsed.set(Duration::from_secs(1));Ok(synthetic_response(200))}).err().unwrap();
    assert!(error.downcast_ref::<RequestTimeout>().is_some());
 }
 #[test]fn get_retry_does_not_retry_mutations_auth_redirects_or_transport_errors() {
    for method in ["POST","PUT","DELETE","GET"] {
        for status in [401,403,429,301,500,503] {
            if method=="GET" && status==503 {continue;}
            let mut clock=simulated_clock();let deadline=clock.start+Duration::from_secs(60);let mut calls=0;
            assert_eq!(retry_get_response(method,deadline,&mut clock,&mut |_|{calls+=1;Ok(synthetic_response(status))}).unwrap().status,status);
            assert_eq!(calls,1);assert!(clock.sleeps.is_empty());
        }
    }
    let mut clock=simulated_clock();let deadline=clock.start+Duration::from_secs(60);let mut calls=0;
    assert!(retry_get_response("GET",deadline,&mut clock,&mut |_|{calls+=1;anyhow::bail!("synthetic invalid response")}).is_err());
    assert_eq!(calls,1);assert!(clock.sleeps.is_empty());
 }
 fn status_server(statuses:Vec<u32>,path:&str)->(String,std::thread::JoinHandle<Vec<String>>) {
    use std::io::{Read,Write};
    let listener=std::net::TcpListener::bind("127.0.0.1:0").unwrap();listener.set_nonblocking(true).unwrap();
    let url=format!("http://{}{path}",listener.local_addr().unwrap());
    let worker=std::thread::spawn(move|| {
        let mut methods=Vec::new();
        for status in statuses {
            let deadline=Instant::now()+Duration::from_secs(15);
            let mut socket=loop {match listener.accept() {
                Ok((socket,_))=>break socket,
                Err(error) if error.kind()==std::io::ErrorKind::WouldBlock && Instant::now()<deadline=>std::thread::sleep(Duration::from_millis(5)),
                _=>return methods,
            }};
            socket.set_read_timeout(Some(Duration::from_secs(15))).unwrap();let mut bytes=Vec::new();let mut part=[0u8;1024];
            loop {
                let n=match socket.read(&mut part) {Ok(0)|Err(_)=>return methods,Ok(n)=>n};bytes.extend_from_slice(&part[..n]);assert!(bytes.len()<=16384);
                if let Some(end)=bytes.windows(4).position(|part|part==b"\r\n\r\n") {
                    let header=std::str::from_utf8(&bytes[..end]).unwrap();
                    let size=header.lines().find_map(|line|line.split_once(':').filter(|(name,_)|name.eq_ignore_ascii_case("content-length")).map(|(_,size)|size.trim().parse::<usize>().unwrap())).unwrap_or(0);
                    if bytes.len()>=end+4+size {methods.push(header.split_whitespace().next().unwrap().to_string());break;}
                }
            }
            let response=format!("HTTP/1.1 {status} Synthetic\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok");
            socket.write_all(response.as_bytes()).unwrap();
        }methods
    });(url,worker)
 }
 #[test]fn production_http_wrappers_retry_get_503_but_not_post_or_get_401() {
    let (url,worker)=status_server(vec![503,200],"/inventory");
    let result=http_request_with_budget("GET",&url,&[],&[],8,Duration::from_secs(15));let methods=worker.join().unwrap();
    assert_eq!(result.unwrap(),b"ok");assert_eq!(methods,vec!["GET","GET"]);
    for (method,status) in [("POST",503),("PUT",503),("DELETE",503),("GET",401),("GET",403)] {
        let (url,worker)=status_server(vec![status],"/single");
        let result=http_request_with_budget(method,&url,&[],&[],8,Duration::from_secs(15));let methods=worker.join().unwrap();
        assert_eq!(result.unwrap_err().downcast_ref::<HttpStatus>().unwrap().0,status);assert_eq!(methods,vec![method]);
    }
    let (url,worker)=status_server(vec![503,200],&format!("/v1/bundles/{}","a".repeat(64)));
    let result=http_download_bundle_with_budget(&url,&[],8,Duration::from_secs(15));let methods=worker.join().unwrap();
    assert_eq!(result.unwrap(),b"ok");assert_eq!(methods,vec!["GET","GET"]);
    // Insufficient budget cannot buy another network request after backoff.
    let (url,worker)=status_server(vec![503],"/deadline");
    let result=http_request_with_budget("GET",&url,&[],&[],8,Duration::from_millis(200));let methods=worker.join().unwrap();
    assert!(result.unwrap_err().downcast_ref::<RequestTimeout>().is_some());assert_eq!(methods,vec!["GET"]);
 }
 #[test]fn exhausted_recovery_budget_fails_before_any_network_request(){
    let ordinary=http_request_with_budget("GET","invalid URL",&[],&[],64,Duration::ZERO).unwrap_err();
    let bundle=http_download_bundle_with_budget("invalid URL",&[],64,Duration::ZERO).unwrap_err();
    assert!(ordinary.downcast_ref::<RequestTimeout>().is_some());
    assert!(bundle.downcast_ref::<RequestTimeout>().is_some());
 }
 #[test]fn apple_installer_redirects_reject_foreign_and_insecure_origins(){for url in ["http://www.apple.com/a","https://apple.com.evil.test/a","https://evil.test/apple.com/a","https://www.apple.com:444/a","https://u:p@www.apple.com/a"]{assert!(!apple_download_url(url));}for url in ["https://www.apple.com/itunes/download/win64/","https://secure-appldnld.apple.com/iTunes64Setup.exe"]{assert!(apple_download_url(url));}}

 #[test]fn bounded_transfer_accepts_slow_complete_body_and_rejects_expired_download(){
    use std::io::{Read,Write};
    fn slow_server()->(String,std::thread::JoinHandle<()>) {
        let listener=std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url=format!("http://{}/slow",listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let worker=std::thread::spawn(move||{
            let deadline=Instant::now()+Duration::from_secs(3);
            let mut socket=loop {
                match listener.accept() {
                    Ok((socket,_))=>break socket,
                    Err(error) if error.kind()==std::io::ErrorKind::WouldBlock && Instant::now()<deadline=>std::thread::sleep(Duration::from_millis(10)),
                    _=>return,
                }
            };socket.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
            let mut request=[0u8;4096];let _=socket.read(&mut request);
            if socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nConnection: close\r\n\r\n").is_err(){return;}
            for byte in b"abcde" {std::thread::sleep(Duration::from_millis(80));if socket.write_all(&[*byte]).is_err(){break;}}
        });(url,worker)
    }
    let(url,worker)=slow_server();
    let result=request("GET",&url,&[],&[],5,Duration::from_millis(200));worker.join().unwrap();
    let error=match result {Err(error)=>error,Ok(_)=>panic!("expired download accepted")};assert!(error.downcast_ref::<RequestTimeout>().is_some());
    let(url,worker)=slow_server();let response=request("GET",&url,&[],&[],5,Duration::from_secs(3)).unwrap();worker.join().unwrap();
    assert_eq!(response.body,b"abcde");assert_eq!(response.status,200);
 }

 #[test]fn bundle_download_rejects_other_routes_and_oversized_caps_without_network(){for url in ["https://example.com/v1/auth/session","https://example.com/v1/bundles/nope"] {assert!(http_download_bundle(url,&[],1).is_err());}assert!(http_download_bundle(&format!("https://example.com/v1/bundles/{}","a".repeat(64)),&[],101*1024*1024).is_err());}

 #[test]fn status_receiver_drains_final_message_and_releases_disconnected_worker(){let(tx,rx)=channel();let mut status=String::new();assert!(!poll_status(&rx,&mut status));tx.send("Comprobando".into()).unwrap();assert!(!poll_status(&rx,&mut status));assert_eq!(status,"Comprobando");tx.send("Actualización preparada".into()).unwrap();drop(tx);assert!(poll_status(&rx,&mut status));assert_eq!(status,"Actualización preparada");}
 #[test]fn status_receiver_can_be_checked_again_after_completion(){let(tx,rx)=channel();drop(tx);let mut status=String::from("Anterior");assert!(poll_status(&rx,&mut status));assert_eq!(status,"Anterior");let(tx,next)=channel();tx.send("Nueva comprobación".into()).unwrap();drop(tx);assert!(poll_status(&next,&mut status));assert_eq!(status,"Nueva comprobación");}
 #[test]fn manifest_rejects_non_main_source_branch_and_foreign_repo(){let base=fixture().read().unwrap();let mut bad=base.clone();bad.source_branch="feature-branch".into();assert!(validate_manifest(&bad).is_err());bad=base;bad.repository="other/repo".into();assert!(validate_manifest(&bad).is_err());}
 #[test]fn transport_rejects_plain_remote_credentials_controls_and_invalid_ports(){for u in["http://example.com/a","https://u:p@example.com/a","https://x:0/a","https://x/a\r\nX:y","file:///x"]{assert!(parse_url(u).is_err());}assert!(parse_url("http://127.0.0.1:8766/v1/auth/login").is_ok());}
 #[test]fn versions_are_numeric_and_do_not_accept_extra_segments(){assert!(version("0.10.0").unwrap()>version("0.6.0").unwrap());assert!(version("0.6.0.1").is_err());assert!(version("x.6.0").is_err());}
 #[test]fn newer_versions_accept_release_build_ids_after_local_date_ids(){
    let mut previous=fixture().read().unwrap();previous.version="0.6.23".into();previous.build_id=2026102701;
    let mut next=previous.clone();next.version="0.6.24".into();next.build_id=42;next.commit="1".repeat(40);
    assert!(require_monotonic(&previous,&next).is_ok());
    assert!(require_monotonic(&next,&previous).is_err());
    let mut same=previous.clone();same.build_id=42;assert!(require_monotonic(&previous,&same).is_err());
 }
 #[test]fn updates_cannot_roll_back_or_redefine_a_build(){let first=fixture().read().unwrap();let mut next=first.clone();next.version="0.0.0".into();assert!(require_monotonic(&first,&next).is_err());next=first.clone();next.build_id=0;assert!(require_monotonic(&first,&next).is_err());next=first.clone();next.commit="1".repeat(40);assert!(require_monotonic(&first,&next).is_err());next.build_id=2;assert!(require_monotonic(&first,&next).is_ok());}
 #[test]fn hash_matches_standard_vector(){assert_eq!(sha256(b"abc").unwrap(),"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");}
 fn fixture()->SignedManifest{SignedManifest{manifest_bytes:include_bytes!("../../test-assets/release-manifest.json").to_vec(),signature:include_bytes!("../../test-assets/release-manifest.sig").to_vec()}}
 #[test]fn publisher_signature_accepts_fixture_and_rejects_tampering(){let signed=fixture();assert_eq!(signed.read().unwrap().version,"0.0.1");let mut bad=signed.clone();bad.manifest_bytes[10]^=1;assert!(bad.read().is_err());let mut bad=signed;bad.signature[0]^=1;assert!(bad.read().is_err());assert!(verify_signature(b"x",&[]).is_err());}
 #[test]fn signed_package_stages_idempotently_and_cache_tampering_is_rejected(){let root=std::env::temp_dir().join(format!("ws-update-test-{}",crate::flasher::generate_token()));let signed=fixture();let bytes=include_bytes!("../../test-assets/update-fixture.zip");stage_at(&root,&signed,bytes).unwrap();stage_at(&root,&signed,bytes).unwrap();assert_eq!(active_at(&root).unwrap().unwrap().build_id,1);let mut bad=bytes.to_vec();bad[0]^=1;assert!(stage_at(&root,&signed,&bad).is_err());std::fs::write(root.join("active.json"),b"{}").unwrap();assert!(active_at(&root).is_err());assert!(root.starts_with(std::env::temp_dir()));std::fs::remove_dir_all(root).unwrap();}
 #[test]fn staging_lock_and_modified_installed_files_fail_closed(){let root=std::env::temp_dir().join(format!("ws-update-test-{}",crate::flasher::generate_token()));let signed=fixture();let bytes=include_bytes!("../../test-assets/update-fixture.zip");std::fs::create_dir_all(&root).unwrap();std::fs::write(root.join("staging.lock"),b"other process fixture").unwrap();assert!(stage_at(&root,&signed,bytes).is_err());assert!(!root.join("active.json").exists());std::fs::remove_file(root.join("staging.lock")).unwrap();stage_at(&root,&signed,bytes).unwrap();std::fs::write(root.join("build-1/Wallet Studio.exe"),b"MZ modified fixture").unwrap();assert!(stage_at(&root,&signed,bytes).is_err());assert!(root.starts_with(std::env::temp_dir()));std::fs::remove_dir_all(root).unwrap();}
 fn archive(names:&[&str])->Vec<u8>{use std::io::Write;let mut w=zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));for name in names{w.start_file(*name,zip::write::SimpleFileOptions::default()).unwrap();w.write_all(b"MZ fixture").unwrap();}w.finish().unwrap().into_inner()}
 #[test]fn package_paths_private_data_and_case_duplicates_are_rejected(){for names in [vec!["Wallet-Studio-0.0.1/../outside"],vec!["Wallet-Studio-0.0.1/bank-backups/x"],vec!["Wallet-Studio-0.0.1/cloud-session.dpapi"],vec!["Wallet-Studio-0.0.1/X","Wallet-Studio-0.0.1/x"],vec!["other/Wallet Studio.exe"],vec!["Wallet-Studio-0.0.1/C:/evil"]]{let bytes=archive(&names);let mut zip=zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();assert!(unpack(&mut zip,"0.0.1").is_err());}}
}
