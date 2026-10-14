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
fn request(method:&str,value:&str,headers:&[(String,String)],body:&[u8],max:usize)->Result<Response>{
    let url=parse_url(value)?;
    anyhow::ensure!(matches!(method,"GET"|"POST"|"PUT"|"DELETE")&&max<=MAX_PACKAGE&&body.len()<=MAX_PACKAGE,"Petición fuera de límites");
    let mut header=String::new();
    for (k,v) in headers {anyhow::ensure!(!k.is_empty()&&k.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'-')&&!v.chars().any(|c|c.is_control()),"Cabecera no válida");header.push_str(&format!("{k}: {v}\r\n"));}
    let deadline=Instant::now()+Duration::from_secs(60);
    let agent=wide("Wallet Studio/0.6");let host=wide(&url.host);let path=wide(&url.path);let verb=wide(method);let hw=wide(&header);
    let session=Handle(unsafe{WinHttpOpen(agent.as_ptr(),0,std::ptr::null(),std::ptr::null(),0)});
    anyhow::ensure!(!session.0.is_null(),"No se pudo iniciar la conexión");
    let connection=Handle(unsafe{WinHttpConnect(session.0,host.as_ptr(),url.port,0)});
    anyhow::ensure!(!connection.0.is_null(),"No se pudo conectar con el servicio");
    let req=Handle(unsafe{WinHttpOpenRequest(connection.0,verb.as_ptr(),path.as_ptr(),std::ptr::null(),std::ptr::null(),std::ptr::null(),if url.secure{0x00800000}else{0})});
    anyhow::ensure!(!req.0.is_null(),"No se pudo crear la petición");
    let mut disable=8u32;
    anyhow::ensure!(unsafe{WinHttpSetOption(req.0,63,(&mut disable as *mut u32).cast(),4)}!=0,"No se pudo bloquear la redirección");
    let timeout=|h|->Result<()>{let ms=deadline.saturating_duration_since(Instant::now()).as_millis();anyhow::ensure!(ms>0,"El servicio agotó el tiempo de espera");let t=ms.min(15000) as i32;anyhow::ensure!(unsafe{WinHttpSetTimeouts(h,t,t,t,t)}!=0,"No se pudo configurar el tiempo de espera");Ok(())};
    timeout(req.0)?;
    anyhow::ensure!(unsafe{WinHttpSendRequest(req.0,hw.as_ptr(),header.encode_utf16().count() as u32,if body.is_empty(){std::ptr::null_mut()}else{body.as_ptr() as *mut _},body.len() as u32,body.len() as u32,0)}!=0,"No se pudo enviar la petición");
    timeout(req.0)?;
    anyhow::ensure!(unsafe{WinHttpReceiveResponse(req.0,std::ptr::null_mut())}!=0,"No se recibió respuesta");
    let mut status=0u32;let mut len=4;
    anyhow::ensure!(unsafe{WinHttpQueryHeaders(req.0,19|0x20000000,std::ptr::null(),(&mut status as *mut u32).cast(),&mut len,std::ptr::null_mut())}!=0,"Estado HTTP no disponible");
    let location=if matches!(status,301|302|303|307|308){let mut buf=[0u16;8192];let mut n=(buf.len()*2)as u32;let ok=unsafe{WinHttpQueryHeaders(req.0,33,std::ptr::null(),buf.as_mut_ptr().cast(),&mut n,std::ptr::null_mut())};if ok!=0{Some(String::from_utf16(&buf[..buf.iter().position(|x|*x==0).unwrap_or(buf.len())])?)}else{None}}else{None};
    let mut out=Vec::new();let mut buffer=[0u8;8192];
    loop{timeout(req.0)?;let mut n=0;anyhow::ensure!(unsafe{WinHttpReadData(req.0,buffer.as_mut_ptr().cast(),8192,&mut n)}!=0,"Lectura de red incompleta");if n==0{break;}anyhow::ensure!(out.len().saturating_add(n as usize)<=max,"Respuesta demasiado grande");out.extend_from_slice(&buffer[..n as usize]);}
    anyhow::ensure!(Instant::now()<deadline,"El servicio agotó el tiempo de espera");
    Ok(Response{status,body:out,location})
}
pub fn http_request(method:&str,url:&str,headers:&[(String,String)],body:&[u8],max_bytes:usize)->Result<Vec<u8>>{
    let r=request(method,url,headers,body,max_bytes)?;
    anyhow::ensure!(matches!(r.status,200|201|204),"El servicio respondió HTTP {}",r.status);
    Ok(r.body)
}
fn public_download(url:&str,max:usize)->Result<Vec<u8>>{
    let mut next=url.to_owned();
    for _ in 0..4{let p=parse_url(&next)?;anyhow::ensure!(p.secure&&matches!(p.host.as_str(),"github.com"|"release-assets.githubusercontent.com"|"objects.githubusercontent.com"),"Origen de actualización no permitido");let r=request("GET",&next,&[],&[],max)?;if r.status==200{return Ok(r.body);}anyhow::ensure!(matches!(r.status,301|302|303|307|308),"Descarga respondió HTTP {}",r.status);next=r.location.ok_or_else(||anyhow::anyhow!("Redirección incompleta"))?;}
    bail!("Demasiadas redirecciones de actualización")
}
#[link(name="bcrypt")]
unsafe extern "system"{fn BCryptOpenAlgorithmProvider(h:*mut *mut std::ffi::c_void,n:*const u16,i:*const u16,f:u32)->i32;fn BCryptHash(h:*mut std::ffi::c_void,k:*const u8,kn:u32,b:*const u8,bn:u32,o:*mut u8,on:u32)->i32;fn BCryptCloseAlgorithmProvider(h:*mut std::ffi::c_void,f:u32)->i32;}
pub(crate) fn sha256(bytes:&[u8])->Result<String>{let mut h=std::ptr::null_mut();let alg=wide("SHA256");let mut out=[0;32];unsafe{anyhow::ensure!(BCryptOpenAlgorithmProvider(&mut h,alg.as_ptr(),std::ptr::null(),0)==0,"SHA-256 no disponible");let status=BCryptHash(h,std::ptr::null(),0,bytes.as_ptr(),u32::try_from(bytes.len())?,out.as_mut_ptr(),32);BCryptCloseAlgorithmProvider(h,0);anyhow::ensure!(status==0,"No se pudo verificar SHA-256");}Ok(out.iter().map(|b|format!("{b:02x}")).collect())}
#[derive(Serialize,Deserialize,Clone)]
struct Manifest{version:String,build_id:u64,repository:String,source_branch:String,commit:String,asset:String,sha256:String,size:usize,exe_sha256:String}
fn version(value:&str)->Result<[u64;3]>{let p=value.split('.').map(str::parse::<u64>).collect::<std::result::Result<Vec<_>,_>>()?;anyhow::ensure!(p.len()==3,"Versión no válida");Ok([p[0],p[1],p[2]])}
fn validate_manifest(m:&Manifest)->Result<()>{anyhow::ensure!(m.repository==REPO&&m.source_branch=="main"&&m.build_id>0&&m.commit.len()==40&&m.commit.bytes().all(|c|c.is_ascii_hexdigit()),"Publicación fuera del canal autorizado");version(&m.version)?;anyhow::ensure!(m.asset==format!("Wallet-Studio-{}-Windows.zip",m.version)&&m.size>0&&m.size<=MAX_PACKAGE&&[&m.sha256,&m.exe_sha256].iter().all(|h|h.len()==64&&h.bytes().all(|c|c.is_ascii_hexdigit())),"Metadatos de actualización no válidos");Ok(())}
fn cache()->PathBuf{crate::storage::root().join("updates")}
fn active()->Result<Option<Manifest>>{let p=cache().join("active.json");if !p.exists(){return Ok(None);}let m:Manifest=serde_json::from_slice(&std::fs::read(p)?)?;validate_manifest(&m)?;Ok(Some(m))}
pub fn redirect_if_ready()->Result<bool>{
    let Some(m)=active()? else{return Ok(false)};
    if version(&m.version)?<version(crate::update_info::VERSION)?{return Ok(false)}
    let target=cache().join(format!("build-{}",m.build_id)).join("Wallet Studio.exe");
    anyhow::ensure!(target.is_file()&&!std::fs::symlink_metadata(&target)?.file_type().is_symlink(),"Actualización preparada no disponible");
    let hash=sha256(&std::fs::read(&target)?)?;anyhow::ensure!(hash==m.exe_sha256,"El ejecutable actualizado no supera la verificación");
    if sha256(&std::fs::read(std::env::current_exe()?)?)?==hash{return Ok(false)}
    std::process::Command::new(target).spawn()?;Ok(true)
}
fn stage(m:&Manifest,bytes:&[u8])->Result<()>{
    use std::io::{Read,Write};
    validate_manifest(m)?;anyhow::ensure!(bytes.len()==m.size&&sha256(bytes)?==m.sha256,"El paquete no coincide con la publicación");
    let mut zip=zip::ZipArchive::new(std::io::Cursor::new(bytes))?;anyhow::ensure!(zip.len()<=2048,"Demasiados archivos en la actualización");
    let prefix=format!("Wallet-Studio-{}/",m.version);let mut files=Vec::new();let mut total=0u64;let mut names=std::collections::HashSet::new();
    for i in 0..zip.len(){let mut f=zip.by_index(i)?;if f.is_dir(){continue;}let raw=f.name();let relative=raw.strip_prefix(&prefix).ok_or_else(||anyhow::anyhow!("Raíz de paquete inesperada"))?;let path=Path::new(relative);anyhow::ensure!(!relative.contains(['\\',':'])&&path.components().all(|c|matches!(c,std::path::Component::Normal(_)))&&names.insert(relative.to_ascii_lowercase()),"Ruta de paquete no válida");anyhow::ensure!(!matches!(path.file_name().and_then(|n|n.to_str()),Some("cards.json"|"baseline.json"|"verified-original.json"|"manifest.urls"|"remote-artwork.bin"))&&f.unix_mode().is_none_or(|m|m&0o170000!=0o120000),"El paquete contiene datos privados o enlaces");total=total.checked_add(f.size()).ok_or_else(||anyhow::anyhow!("Paquete fuera de límites"))?;anyhow::ensure!(total<=256*1024*1024&&f.size()<=MAX_PACKAGE as u64,"Actualización descomprimida demasiado grande");let name=path.to_owned();let mut data=Vec::new();f.by_ref().take(MAX_PACKAGE as u64+1).read_to_end(&mut data)?;anyhow::ensure!(data.len()as u64==f.size(),"Archivo descomprimido incompleto");files.push((name,data));}
    let exe=files.iter().find(|(p,_)|p==Path::new("Wallet Studio.exe")).ok_or_else(||anyhow::anyhow!("No hay ejecutable en el paquete"))?;
    anyhow::ensure!(exe.1.starts_with(b"MZ")&&sha256(&exe.1)?==m.exe_sha256,"Ejecutable de actualización no válido");
    let root=cache();std::fs::create_dir_all(&root)?;let destination=root.join(format!("build-{}",m.build_id));
    if !destination.exists(){let temp=root.join(format!("staging-{}-{}",m.build_id,crate::flasher::generate_token()));std::fs::create_dir(&temp)?;for(p,b)in &files{let target=temp.join(p);std::fs::create_dir_all(target.parent().unwrap())?;let mut f=std::fs::OpenOptions::new().write(true).create_new(true).open(&target)?;f.write_all(b)?;f.sync_all()?;anyhow::ensure!(std::fs::read(target)?==*b,"Actualización no supera lectura posterior");}anyhow::ensure!(temp.parent()==Some(root.as_path())&&destination.parent()==Some(root.as_path()),"Destino de actualización no válido");std::fs::rename(temp,&destination)?;}
    anyhow::ensure!(sha256(&std::fs::read(destination.join("Wallet Studio.exe"))?)?==m.exe_sha256,"Instalación preparada no coincide");
    let temp=root.join(format!("active-{}.tmp",crate::flasher::generate_token()));let data=serde_json::to_vec(m)?;let mut f=std::fs::OpenOptions::new().write(true).create_new(true).open(&temp)?;f.write_all(&data)?;f.sync_all()?;drop(f);
    // MoveFileEx is atomic and only replaces our update pointer, never backups.
    #[link(name="kernel32")]unsafe extern "system"{fn MoveFileExW(a:*const u16,b:*const u16,f:u32)->i32;}
    let a=wide(&temp.to_string_lossy());let b=wide(&root.join("active.json").to_string_lossy());anyhow::ensure!(unsafe{MoveFileExW(a.as_ptr(),b.as_ptr(),1|8)}!=0,"No se pudo activar la actualización preparada");Ok(())
}
fn check()->Result<String>{
    let url=format!("https://api.github.com/repos/{REPO}/releases/latest");let data=http_request("GET",&url,&[("Accept".into(),"application/vnd.github+json".into())],&[],1024*1024)?;let release:serde_json::Value=serde_json::from_slice(&data)?;
    anyhow::ensure!(release["draft"]==false&&release["prerelease"]==false,"Publicación no estable");let assets=release["assets"].as_array().ok_or_else(||anyhow::anyhow!("Canal sin paquetes"))?;
    let asset_url=|name:&str|->Result<String>{let matches=assets.iter().filter(|a|a["name"].as_str()==Some(name)).collect::<Vec<_>>();anyhow::ensure!(matches.len()==1,"Paquete de publicación ambiguo");let url=matches[0]["browser_download_url"].as_str().ok_or_else(||anyhow::anyhow!("Descarga no disponible"))?;anyhow::ensure!(url.starts_with(&format!("https://github.com/{REPO}/releases/download/")),"Descarga fuera del repositorio");Ok(url.into())};
    let m:Manifest=serde_json::from_slice(&public_download(&asset_url("release-manifest.json")?,65536)?)?;validate_manifest(&m)?;
    let current=version(crate::update_info::VERSION)?;let v=version(&m.version)?;if v<current{return Ok("La versión instalada es más reciente que el canal publicado".into())}
    let previous=active()?;if previous.is_some_and(|p|p.build_id>=m.build_id&&version(&p.version).is_ok_and(|x|x>=v)){return Ok("Wallet Studio está actualizado".into())}
    if v==current&&option_env!("WALLET_STUDIO_BUILD_ID").and_then(|s|s.parse::<u64>().ok()).is_some_and(|b|b>=m.build_id){return Ok("Wallet Studio está actualizado".into())}
    let package=public_download(&asset_url(&m.asset)?,MAX_PACKAGE)?;stage(&m,&package)?;Ok(format!("Actualización {} preparada: se activará al volver a abrir",m.version))
}
pub fn start_background_check()->Receiver<String>{let(tx,rx)=channel();std::thread::spawn(move||{let _=tx.send("Comprobando actualizaciones…".into());let status=match check(){Ok(s)=>s,Err(_)=>"Canal de actualización no disponible; puedes seguir trabajando".into()};let _=tx.send(status);});rx}
#[cfg(test)]mod tests{use super::*;
 #[test]fn transport_rejects_plain_remote_credentials_controls_and_invalid_ports(){for u in["http://example.com/a","https://u:p@example.com/a","https://x:0/a","https://x/a\r\nX:y","file:///x"]{assert!(parse_url(u).is_err());}assert!(parse_url("http://127.0.0.1:8766/v1/auth/login").is_ok());}
 #[test]fn versions_are_numeric_and_do_not_accept_extra_segments(){assert!(version("0.10.0").unwrap()>version("0.6.0").unwrap());assert!(version("0.6.0.1").is_err());assert!(version("x.6.0").is_err());}
 #[test]fn hash_matches_standard_vector(){assert_eq!(sha256(b"abc").unwrap(),"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");}
}
