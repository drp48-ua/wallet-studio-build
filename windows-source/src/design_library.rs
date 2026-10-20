//! Local, bounded image inputs. The library never interprets URLs or manifests.
use anyhow::{Context, Result};
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};

pub const MAX_IMAGE_BYTES: u64 = 32 * 1024 * 1024;
const MAX_PIXELS: u64 = 16 * 1024 * 1024;
pub const MAX_DESIGNS: usize = 100;
pub const MAX_INCLUDED_DESIGNS: usize = 100;
/// Paid catalogue: legacy 320x200 assets keep their 1 MiB bound, native card assets (1536x969) may reach 4 MiB.
const LEGACY_ASSET_BYTES: usize = 1024 * 1024;
const MAX_ASSET_BYTES: usize = 4 * 1024 * 1024;
const LEGACY_DESIGN_SIZE: (u32, u32) = (320, 200);
const NATIVE_DESIGN_SIZE: (u32, u32) = (crate::image_skin::CARD_WIDTH, crate::image_skin::CARD_HEIGHT);
/// Keep in sync with NATIVE_2K_CAPABILITY in server/designs.py.
const NATIVE_CATALOG_CAPABILITY: &str = "native-2k-v2";

#[derive(Clone, Debug)]
pub struct Design {
    pub path: PathBuf,
    pub title: String,
}

pub fn library_dir() -> Result<PathBuf> {
    if crate::storage::is_demo() {return Ok(crate::storage::root().join("demo-library").join("own"));}
    let session=crate::cloud_sync::load_session()?.context("Inicia sesión para abrir tu biblioteca")?;
    let root=account_dir(&session)?.join("own");crate::storage::safe_chain(&root)?;
    Ok(root)
}
/// Copy legacy user saves once, without moving originals or sharing paid cache.
pub fn migrate_legacy_saves(root:&Path)->Result<()> {
    crate::storage::safe_chain(root)?;std::fs::create_dir_all(root)?;
    let marker=root.join(".legacy-migrated");crate::storage::safe_chain(&marker)?;
    if marker.exists() {return Ok(());}
    let legacy=std::env::current_exe()?.parent().context("Carpeta anterior no disponible")?.join("designs");
    for design in discover(&legacy)? {
        if design.path.file_name().and_then(|name|name.to_str()).is_some_and(|name|name.starts_with("guardado-") && name.ends_with(".png")) {
            let png=read_image(&design.path)?;save_prepared_png(root,&png)?;
        }
    }
    crate::storage::write_new(&marker,b"1")?;Ok(())
}

pub fn supported_path(path: &Path) -> bool {
    path.extension().and_then(|value| value.to_str()).is_some_and(|value|
        ["png", "jpg", "jpeg", "webp", "gif"].contains(&value.to_ascii_lowercase().as_str()))
}

pub fn discover(root: &Path) -> Result<Vec<Design>> {
    if !root.exists() { return Ok(Vec::new()); }
    anyhow::ensure!(!std::fs::symlink_metadata(root).context("Inspección de la carpeta de diseños")?.file_type().is_symlink(), "La biblioteca no puede ser un enlace");
    let canonical = root.canonicalize().context("Ruta real de la carpeta de diseños")?;
    let mut ranked = Vec::new();
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if !supported_path(&path) || !entry.file_type()?.is_file() { continue; }
        let metadata = std::fs::symlink_metadata(&path).context("Inspección del archivo de diseño")?;
        if !metadata.is_file() || metadata.file_type().is_symlink() || !supported_path(&path)
            || metadata.len() == 0 || metadata.len() > MAX_IMAGE_BYTES { continue; }
        let resolved = path.canonicalize().context("Ruta real del archivo de diseño")?;
        if resolved.parent() != Some(canonical.as_path()) { continue; }
        let mut title: String = path.file_stem().unwrap_or_default().to_string_lossy()
            .replace(['_', '-'], " ").chars().filter(|c| !c.is_control()).take(80).collect();
        if let Some(id) = path.file_stem().and_then(|stem| stem.to_str()).and_then(|stem| stem.strip_prefix("guardado-")) {
            title = format!("Diseño guardado · {}", id.chars().take(8).collect::<String>());
        }
        ranked.push((metadata.modified().unwrap_or(std::time::UNIX_EPOCH), Design { path: resolved, title }));
        // Bounded storage, but inspect the entire directory so new saves are not hidden.
        ranked.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.path.cmp(&b.1.path)));
        ranked.truncate(MAX_DESIGNS);
    }
    let mut designs: Vec<_> = ranked.into_iter().map(|(_, design)| design).collect();
    designs.sort_by_cached_key(|design| design.title.to_lowercase());
    Ok(designs)
}

/// Save the exact prepared PNG without overwriting any existing design.
/// The content key is for deduplication only; exact byte equality verifies reuse.
pub fn save_prepared_png(root: &Path, png: &[u8]) -> Result<PathBuf> {
    anyhow::ensure!(png.starts_with(b"\x89PNG\r\n\x1a\n"), "El diseño preparado debe ser PNG");
    let image = decode_still(png)?;
    anyhow::ensure!(image.width() == crate::image_skin::CARD_WIDTH && image.height() == crate::image_skin::CARD_HEIGHT,
        "El diseño preparado no tiene el tamaño de tarjeta");
    crate::storage::safe_chain(root)?;
    std::fs::create_dir_all(root).context("No se pudo crear la biblioteca de diseños")?;
    let metadata = std::fs::symlink_metadata(root)?;
    anyhow::ensure!(metadata.is_dir() && !metadata.file_type().is_symlink(), "La biblioteca debe ser una carpeta normal");
    let root = root.canonicalize()?;
    let key = png.iter().fold(0xcbf29ce484222325u64, |hash, byte|
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3));
    for suffix in 0..1000 {
        let name = if suffix == 0 { format!("guardado-{key:016x}.png") }
            else { format!("guardado-{key:016x}-{suffix}.png") };
        let path = root.join(name);
        let file = std::fs::OpenOptions::new().write(true).create_new(true).open(&path);
        match file {
            Ok(mut file) => {
                let saved = (|| -> Result<()> {
                    file.write_all(png)?;
                    file.sync_all()?;
                    anyhow::ensure!(read_image(&path)? == png, "El diseño guardado no pasó la verificación");
                    Ok(())
                })();
                drop(file);
                if let Err(error) = saved {
                    // Only this newly created file is owned by this operation.
                    let _ = std::fs::remove_file(&path);
                    return Err(error).context("No se pudo guardar el diseño en la biblioteca");
                }
                return Ok(path);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                if read_image(&path).is_ok_and(|existing| existing == png) { return Ok(path); }
            }
            Err(error) => return Err(error).context("No se pudo guardar el diseño en la biblioteca"),
        }
    }
    anyhow::bail!("No se pudo reservar un nombre para el diseño")
}

/// A transfer can only start after an exact, durable library copy exists.
pub fn save_before_apply<T>(root: &Path, png: &[u8], apply: impl FnOnce(&Path) -> Result<T>) -> Result<T> {
    let path = save_prepared_png(root, png)
        .context("No se guardó el diseño; no se inició la aplicación en el iPhone")?;
    apply(&path)
}

/// Preserve the prepared transfer durably without duplicating included designs
/// in the user's file-import library. Both paths remain account-scoped.
pub fn save_before_apply_from_source<T>(account: &Path, png: &[u8], from_file: bool,
    apply: impl FnOnce(&Path) -> Result<T>) -> Result<T> {
    let root=account.join(if from_file {"own"} else {"prepared-transfers"});
    save_before_apply(&root,png,|path| {
        if from_file { mark_user_import(path)?; }
        apply(path)
    })
}

fn mark_user_import(path:&Path)->Result<()> {
    let marker=path.with_extension("user-import");crate::storage::safe_chain(&marker)?;
    if marker.exists() {anyhow::ensure!(std::fs::read(&marker)?==b"user-file-v1","Procedencia de importación no válida");return Ok(());}
    crate::storage::write_new(&marker,b"user-file-v1")
}
pub fn save_user_import(root:&Path,png:&[u8])->Result<PathBuf> {
    let path=save_prepared_png(root,png)?;mark_user_import(&path)?;Ok(path)
}
fn prepared_pixel_key(path:&Path)->Result<String> {
    let prepared=crate::image_skin::PreparedSkin::from_image_with_focus(decode_still(&read_image(path)?)?,0.5,0.5)?;
    crate::bank_backup::sha256(decode_still(&prepared.png)?.to_rgba8().as_raw())
}
/// Presentation-only cleanup of old automatic catalog saves. Never delete files,
/// and never hide an explicitly marked import, even if it matches the catalog.
pub fn personal_designs_only(own:Vec<Design>,included:&[(PurchasedDesign,PathBuf)])->Vec<Design> {
    let keys:std::collections::HashSet<_>=included.iter().filter_map(|(item,path)| {
        let bytes=read_image(path).ok()?;verify_purchased(item,&bytes).ok()?;
        prepared_pixel_key(path).ok()
    }).collect();
    own.into_iter().filter(|design| {
        let marker=design.path.with_extension("user-import");
        if crate::storage::safe_chain(&marker).is_ok() && std::fs::read(&marker).is_ok_and(|bytes|bytes==b"user-file-v1") {return true;}
        prepared_pixel_key(&design.path).map_or(true,|key|!keys.contains(&key))
    }).collect()
}

pub fn read_image(path: &Path) -> Result<Vec<u8>> {
    anyhow::ensure!(supported_path(path), "Usa PNG, JPG, WebP o GIF");
    let metadata = std::fs::symlink_metadata(path)?;
    anyhow::ensure!(metadata.is_file() && !metadata.file_type().is_symlink(), "Selecciona un archivo local normal");
    anyhow::ensure!(metadata.len() > 0 && metadata.len() <= MAX_IMAGE_BYTES, "El diseño debe ocupar como máximo 32 MB");
    // Bound the read itself as the file may change after metadata inspection.
    let mut bytes = Vec::new();
    std::fs::File::open(path)?.take(MAX_IMAGE_BYTES + 1).read_to_end(&mut bytes)?;
    anyhow::ensure!(!bytes.is_empty() && bytes.len() as u64 <= MAX_IMAGE_BYTES, "El diseño debe ocupar como máximo 32 MB");
    Ok(bytes)
}

pub fn decode_still(bytes: &[u8]) -> Result<image::DynamicImage> {
    anyhow::ensure!(!bytes.is_empty() && bytes.len() as u64 <= MAX_IMAGE_BYTES, "Imagen demasiado grande");
    let dimensions = image::ImageReader::new(Cursor::new(bytes)).with_guessed_format()?.into_dimensions()?;
    anyhow::ensure!(dimensions.0 > 0 && dimensions.1 > 0
        && u64::from(dimensions.0) * u64::from(dimensions.1) <= MAX_PIXELS,
        "La imagen supera el límite de 16 megapíxeles");
    let mut reader = image::ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(16384);
    limits.max_image_height = Some(16384);
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    Ok(reader.decode()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn prepared_png() -> Vec<u8> {
        let mut png = Vec::new();
        image::DynamicImage::new_rgb8(crate::image_skin::CARD_WIDTH, crate::image_skin::CARD_HEIGHT)
            .write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
        png
    }
    #[test]
    fn catalog_transfer_never_enters_my_designs_and_preserves_failed_transfer_evidence() {
        let root=std::env::temp_dir().join(format!("ws-catalog-transfer-{}",crate::flasher::generate_token()));
        let png=prepared_png();
        let result:Result<()>=save_before_apply_from_source(&root,&png,false,|path| {
            assert_eq!(path.parent().unwrap().canonicalize()?,root.join("prepared-transfers").canonicalize()?);
            assert_eq!(read_image(path)?,png);anyhow::bail!("fixture transfer failure")
        });
        assert!(result.is_err());assert!(!root.join("own").exists());
        assert_eq!(discover(&root.join("prepared-transfers")).unwrap().len(),1);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn explicit_file_import_is_kept_even_when_matching_a_catalog_design() {
        let root=std::env::temp_dir().join(format!("ws-import-origin-{}",crate::flasher::generate_token()));
        let png=prepared_png();
        let path=save_user_import(&root.join("own"),&png).unwrap();
        let item=PurchasedDesign {id:"ws-001".into(),title:"Fixture".into(),filename:"ws-001.png".into(),
            sha256:crate::bank_backup::sha256(&png).unwrap(),bytes:png.len(),width:1536,height:969,thumbnail:None};
        let included=vec![(item,path.clone())];
        assert_eq!(personal_designs_only(discover(&root.join("own")).unwrap(),&included).len(),1);
        std::fs::remove_file(path.with_extension("user-import")).unwrap();
        assert!(personal_designs_only(discover(&root.join("own")).unwrap(),&included).is_empty());
        assert_eq!(read_image(&path).unwrap(),png,"filtering must never delete the old design");
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn user_file_transfer_is_saved_to_own_and_marked_before_transfer() {
        let root=std::env::temp_dir().join(format!("ws-file-transfer-{}",crate::flasher::generate_token()));
        save_before_apply_from_source(&root,&prepared_png(),true,|path| {
            assert_eq!(path.parent().unwrap().canonicalize()?,root.join("own").canonicalize()?);
            assert_eq!(std::fs::read(path.with_extension("user-import"))?,b"user-file-v1");Ok(())
        }).unwrap();
        assert!(!root.join("prepared-transfers").exists());std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn automatically_saved_design_is_exact_reusable_and_survives_a_failed_transfer() {
        let root = std::env::temp_dir().join(format!("wallet-autosave-{}", std::process::id()));
        let png = prepared_png();
        let result: Result<()> = save_before_apply(&root, &png, |path| {
            assert_eq!(read_image(path)?, png);
            anyhow::bail!("simulated transport failure")
        });
        assert!(result.is_err());
        let saved = save_prepared_png(&root, &png).unwrap();
        assert_eq!(discover(&root).unwrap().len(), 1);
        assert_eq!(read_image(&saved).unwrap(), png);
        assert_eq!(save_prepared_png(&root, &png).unwrap(), saved);
        std::fs::remove_file(saved).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
    #[test]
    fn library_error_blocks_transfer_without_replacing_existing_data() {
        let root = std::env::temp_dir().join(format!("wallet-autosave-blocked-{}", std::process::id()));
        std::fs::write(&root, b"preserve this file").unwrap();
        let mut called = false;
        assert!(save_before_apply(&root, &prepared_png(), |_| { called = true; Ok(()) }).is_err());
        assert!(!called);
        assert_eq!(std::fs::read(&root).unwrap(), b"preserve this file");
        std::fs::remove_file(root).unwrap();
    }
    #[test]
    fn occupied_design_filename_is_preserved_and_another_name_is_reserved() {
        let root = std::env::temp_dir().join(format!("wallet-autosave-occupied-{}", std::process::id()));
        let png = prepared_png();
        let first = save_prepared_png(&root, &png).unwrap();
        std::fs::write(&first, b"existing file must survive").unwrap();
        let second = save_prepared_png(&root, &png).unwrap();
        assert_ne!(first, second);
        assert_eq!(std::fs::read(&first).unwrap(), b"existing file must survive");
        assert_eq!(read_image(&second).unwrap(), png);
        std::fs::remove_file(first).unwrap();
        std::fs::remove_file(second).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
    #[test]
    fn library_only_lists_direct_supported_regular_images() {
        let dir = std::env::temp_dir().join(format!("wallet-library-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("nested")).unwrap();
        std::fs::write(dir.join("02_blue.PNG"), b"sample").unwrap();
        std::fs::write(dir.join("01-green.webp"), b"sample").unwrap();
        std::fs::write(dir.join("run.exe"), b"sample").unwrap();
        std::fs::write(dir.join("nested/hidden.png"), b"sample").unwrap();
        let designs = discover(&dir).unwrap();
        assert_eq!(designs.iter().map(|d| d.title.as_str()).collect::<Vec<_>>(), ["01 green", "02 blue"]);
        assert!(designs.iter().all(|d| d.path.parent() == Some(dir.canonicalize().unwrap().as_path())));
        for path in [dir.join("02_blue.PNG"), dir.join("01-green.webp"), dir.join("run.exe"), dir.join("nested/hidden.png")] { std::fs::remove_file(path).unwrap(); }
        std::fs::remove_dir(dir.join("nested")).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }
    #[test]
    fn rejects_unsupported_inputs_and_pixel_bombs() {
        assert!(!supported_path(Path::new("image.svg")));
        assert!(!supported_path(Path::new("run.png.exe")));
        assert!(supported_path(Path::new("design.JPEG")));
        let mut png = Vec::new();
        image::DynamicImage::new_rgb8(1, 1).write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
        assert_eq!(decode_still(&png).unwrap().width(), 1);
        assert!(decode_still(b"not an image").is_err());
        png[16..20].copy_from_slice(&32768u32.to_be_bytes());
        png[20..24].copy_from_slice(&32768u32.to_be_bytes());
        let mut crc = !0u32;
        for byte in &png[12..29] {
            crc ^= u32::from(*byte);
            for _ in 0..8 { crc = (crc >> 1) ^ (0xedb88320 & (0u32.wrapping_sub(crc & 1))); }
        }
        png[29..33].copy_from_slice(&(!crc).to_be_bytes());
        assert_eq!(image::ImageReader::new(Cursor::new(&png)).with_guessed_format().unwrap().into_dimensions().unwrap(), (32768, 32768));
        assert!(decode_still(&png).unwrap_err().to_string().contains("16 megapíxeles"));
    }
}


#[derive(Clone,Debug,serde::Deserialize)]
pub struct PurchasedDesign { pub id:String,pub title:String,pub filename:String,pub sha256:String,pub bytes:usize,pub width:u32,pub height:u32,#[serde(default)] pub thumbnail:Option<Box<PurchasedDesign>> }
#[derive(serde::Deserialize)]
pub struct PurchasedCatalog { pub schema:u32,pub pack_id:String,pub entitled:bool,pub designs:Vec<PurchasedDesign> }
/// Per-profile bound from the shared catalogue contract; unknown geometry is rejected.
fn asset_limit(width:u32,height:u32)->Option<usize> {
    if (width,height)==LEGACY_DESIGN_SIZE {Some(LEGACY_ASSET_BYTES)}
    else if native_geometry(width,height) {Some(MAX_ASSET_BYTES)}
    else {None}
}
pub fn native_geometry(width:u32,height:u32)->bool { (640..=4096).contains(&width) && (400..=4096).contains(&height) && u64::from(width)*u64::from(height)<=16*1024*1024 }
pub fn account_dir(session:&crate::cloud_sync::Session)->Result<PathBuf> {
    anyhow::ensure!(crate::cloud_sync::valid_server(&session.server) && !session.user_id.is_empty(),"Cuenta no válida");
    let key=crate::bank_backup::sha256(format!("{}|{}",session.server,session.user_id).as_bytes())?;
    let root=crate::storage::root().join("design-library").join(key);crate::storage::safe_chain(&root)?;Ok(root)
}
pub fn purchased_catalog(session:&crate::cloud_sync::Session)->Result<PurchasedCatalog> {
    anyhow::ensure!(crate::cloud_sync::valid_server(&session.server),"Servicio no válido");
    let bytes=design_request(session,"/v1/designs",65536)
        .context("No se pudo actualizar el pack: revisa tu conexión o actualiza el servidor")?;
    parse_purchased_catalog(&bytes)
}
fn parse_purchased_catalog(bytes:&[u8])->Result<PurchasedCatalog> {
    anyhow::ensure!(bytes.len()<=65536,"Catálogo demasiado grande");
    let catalog:PurchasedCatalog=serde_json::from_slice(&bytes)?;
    anyhow::ensure!(catalog.schema==1 && catalog.pack_id=="studio-50-v1" && ((!catalog.entitled && catalog.designs.is_empty()) || (catalog.entitled && (50..=MAX_INCLUDED_DESIGNS).contains(&catalog.designs.len()))),"Catálogo no compatible");
    for (index,item) in catalog.designs.iter().enumerate() {
        anyhow::ensure!(item.id==format!("ws-{:03}",index+1) && item.filename==format!("{}.png",item.id) && item.sha256.len()==64 && item.sha256.bytes().all(|b|b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) && item.bytes>0 && asset_limit(item.width,item.height).is_some_and(|limit|item.bytes<=limit) && !item.title.is_empty() && item.title.len()<=320 && !item.title.chars().any(char::is_control),"Metadatos de diseño no válidos");
        if let Some(preview)=&item.thumbnail {
            anyhow::ensure!(preview.thumbnail.is_none() && preview.id==item.id && preview.filename==item.filename && (preview.width,preview.height)==LEGACY_DESIGN_SIZE && preview.bytes>0 && preview.bytes<=LEGACY_ASSET_BYTES && preview.sha256.len()==64 && preview.sha256.bytes().all(|b|b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),"Miniatura no válida");
        }
    } Ok(catalog)
}
pub fn verify_purchased(item:&PurchasedDesign,bytes:&[u8])->Result<()> {
    let limit=asset_limit(item.width,item.height).context("Tamaño de diseño no válido")?;
    anyhow::ensure!(bytes.len()==item.bytes && bytes.len()<=limit && bytes.starts_with(b"\x89PNG\r\n\x1a\n") && crate::bank_backup::sha256(bytes)?==item.sha256,"El diseño descargado no pasó la verificación");
    let image=decode_still(bytes)?;anyhow::ensure!((image.width(),image.height())==(item.width,item.height),"Tamaño de diseño no válido");Ok(())
}
pub fn download_purchased(session:&crate::cloud_sync::Session,item:&PurchasedDesign)->Result<PathBuf> {
    anyhow::ensure!(native_geometry(item.width,item.height),"Este diseño aún no tiene un original de alta resolución publicado. Actualiza la biblioteca; no se ampliará la miniatura.");
    anyhow::ensure!(crate::cloud_sync::valid_server(&session.server) && item.id.len()==6 && item.id.starts_with("ws-") && item.id[3..].bytes().all(|b|b.is_ascii_digit()),"Diseño no válido");
    // Always ask the service when using a paid design, including cached designs.
    let bytes=design_request(session,&format!("/v1/designs/{}",item.id),MAX_ASSET_BYTES)?;
    verify_purchased(item,&bytes)?;
    let path=purchased_cache_path(session,item)?;
    store_purchased_cache(&path,item,&bytes)?;
    Ok(path)
}
static DESIGN_NETWORK:std::sync::Mutex<()>=std::sync::Mutex::new(());
fn design_request(session:&crate::cloud_sync::Session,path:&str,maximum:usize)->Result<Vec<u8>> {
    let _network=DESIGN_NETWORK.lock().map_err(|_|anyhow::anyhow!("La conexión de diseños necesita reiniciarse"))?;
    for attempt in 0..3 {
        let result=crate::auto_update::http_request("GET",&format!("{}{}",session.server,path),&[("Authorization".into(),format!("Bearer {}",session.access_token)),("X-Wallet-Studio-Catalog".into(),NATIVE_CATALOG_CAPABILITY.into())],&[],maximum);
        let retry=result.as_ref().err().is_some_and(|error|error.downcast_ref::<crate::auto_update::HttpStatus>().map_or(true,|status|matches!(status.0,429|503)));
        if !retry || attempt==2 {return result;}
        std::thread::sleep(std::time::Duration::from_millis(400*(attempt+1)));
    }
    unreachable!()
}
pub fn cached_thumbnail(session:&crate::cloud_sync::Session,item:&PurchasedDesign)->Result<PathBuf> {
    let preview=item.thumbnail.as_deref().unwrap_or(item);
    let path=purchased_cache_path(session,preview)?;
    if path.exists() && read_image(&path).is_ok_and(|bytes|verify_purchased(preview,&bytes).is_ok()) {return Ok(path);}
    let suffix=if item.thumbnail.is_some() {"/thumbnail"} else {""};
    let bytes=design_request(session,&format!("/v1/designs/{}{}",item.id,suffix),MAX_ASSET_BYTES)?;
    store_purchased_cache(&path,preview,&bytes)?;Ok(path)
}
fn store_purchased_cache(path:&Path,item:&PurchasedDesign,bytes:&[u8])->Result<()> {
    verify_purchased(item,bytes)?;crate::storage::safe_chain(path)?;
    let root=path.parent().context("Carpeta de caché no válida")?;std::fs::create_dir_all(root)?;
    if read_image(path).is_ok_and(|old|old==bytes) {return Ok(());}
    // Atomic replacement prevents another preview/use worker seeing a partial PNG.
    let temporary=root.join(format!(".design-{}",crate::flasher::generate_token()));
    crate::storage::write_new(&temporary,bytes)?;
    let result=(|| {
        verify_purchased(item,&std::fs::read(&temporary)?)?;
        crate::storage::replace_file_at(&temporary,path)?;
        verify_purchased(item,&read_image(path)?)
    })();
    if result.is_err() {let _=std::fs::remove_file(&temporary);}
    result
}
pub fn purchased_cache_path(session:&crate::cloud_sync::Session,item:&PurchasedDesign)->Result<PathBuf> {
    anyhow::ensure!(item.sha256.len()==64 && item.sha256.bytes().all(|b|b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),"Hash de diseño no válido");
    let path=account_dir(session)?.join("purchased").join(format!("{}.png",item.sha256));crate::storage::safe_chain(&path)?;Ok(path)
}
pub fn cached_purchased(session:&crate::cloud_sync::Session,item:&PurchasedDesign)->Result<PathBuf> {
    let path=purchased_cache_path(session,item)?;
    if path.exists() && read_image(&path).is_ok_and(|bytes|verify_purchased(item,&bytes).is_ok()) {return Ok(path);}
    download_purchased(session,item)
}

/// Design-only read diagnostic: no migration, cache mutation, identity output,
/// Apple initialization or phone operations. Credentials never leave Session.
pub fn diagnose_account_designs(all:bool)->serde_json::Value {
    let session=match crate::cloud_sync::load_session() {
        Ok(Some(session))=>session,Ok(None)=>return serde_json::json!({"result":"no_saved_session"}),
        Err(_)=>return serde_json::json!({"result":"session_unavailable"}),
    };
    let catalog=match purchased_catalog(&session) {Ok(catalog)=>catalog,Err(_)=>return serde_json::json!({"result":"catalog_unavailable"})};
    if all {
        let started=std::time::Instant::now();let mut thumbnails=0;let mut originals=0;let mut failures=std::collections::BTreeMap::<String,usize>::new();let mut attempted=0;
        let expected=catalog.designs.len()+catalog.designs.iter().filter(|item|native_geometry(item.width,item.height)).count();
        'items:for item in &catalog.designs {
            let preview=item.thumbnail.as_deref().unwrap_or(item);
            let mut requests=vec![(preview,format!("/v1/designs/{}{}",item.id,if item.thumbnail.is_some() {"/thumbnail"} else {""}),false)];
            if native_geometry(item.width,item.height) {requests.push((item,format!("/v1/designs/{}",item.id),true));}
            for (metadata,path,native) in requests {
                if started.elapsed().as_secs()>=240 {break 'items;}
                attempted+=1;
                match design_request(&session,&path,MAX_ASSET_BYTES).and_then(|bytes|verify_purchased(metadata,&bytes)) {
                    Ok(())=>if native {originals+=1} else {thumbnails+=1},
                    Err(error)=>{let category=error.downcast_ref::<crate::auto_update::HttpStatus>().map(|status|format!("http_{}",status.0)).unwrap_or_else(||"transport_or_integrity".into());*failures.entry(category).or_default()+=1;}
                }
            }
        }
        return serde_json::json!({"result":if attempted==expected {"complete"} else {"time_budget_reached"},"catalog_count":catalog.designs.len(),"expected_requests":expected,"attempted":attempted,"verified_thumbnails":thumbnails,"verified_native":originals,"native_missing":catalog.designs.iter().filter(|item|!native_geometry(item.width,item.height)).count(),"failures":failures});
    }
    let mut checks=Vec::new();
    for id in ["ws-001","ws-026","ws-030","ws-056"] {
        let Some(item)=catalog.designs.iter().find(|item|item.id==id) else {checks.push(serde_json::json!({"id":id,"result":"not_in_catalog"}));continue;};
        let result=match design_request(&session,&format!("/v1/designs/{id}"),MAX_ASSET_BYTES) {
            Ok(bytes)=>if verify_purchased(item,&bytes).is_ok() {"verified".to_string()} else {"integrity_failed".into()},
            Err(error)=>error.downcast_ref::<crate::auto_update::HttpStatus>().map(|status|format!("http_{}",status.0)).unwrap_or_else(||"transport_failed".into()),
        };
        checks.push(serde_json::json!({"id":id,"width":item.width,"height":item.height,"result":result}));
    }
    serde_json::json!({"result":"complete","catalog_count":catalog.designs.len(),"native_count":catalog.designs.iter().filter(|item|native_geometry(item.width,item.height)).count(),"checks":checks})
}

#[cfg(test)]
mod paid_tests {
    use super::*;
    /// Explicit host-only diagnostic. No storage initialization, cache writes,
    /// identity output, raw errors, Apple initialization or device operations.
    #[test]
    #[ignore = "requires explicit authorized saved-account design-only host diagnostic"]
    fn diagnose_live_design_failures_read_only() {
        if std::env::var("WALLET_STUDIO_DESIGN_DIAGNOSTIC").as_deref()!=Ok("1") {println!("{{\"result\":\"explicit_diagnostic_required\"}}");return;}
        let session=match crate::cloud_sync::load_session() {Ok(Some(session))=>session,_=>{println!("{{\"result\":\"session_unavailable\"}}");return;}};
        let catalog=match purchased_catalog(&session) {Ok(catalog)=>catalog,_=>{println!("{{\"result\":\"catalog_unavailable\"}}");return;}};
        let started=std::time::Instant::now();let mut checks=Vec::new();
        'items:for item in &catalog.designs {
            let preview=item.thumbnail.as_deref().unwrap_or(item);
            for (metadata,native,suffix) in [(preview,false,if item.thumbnail.is_some(){"/thumbnail"}else{""}),(item,true,"")] {
                if checks.len()>=112 || started.elapsed().as_secs()>=240 {break 'items;}
                let path=format!("/v1/designs/{}{}",item.id,suffix);
                let category=match design_request(&session,&path,MAX_ASSET_BYTES) {
                    Ok(bytes)=>if verify_purchased(metadata,&bytes).is_ok(){"verified"}else{"integrity"},
                    Err(error)=>if error.chain().any(|cause|matches!(cause.to_string().as_str(),"Lectura de red incompleta"|"Respuesta demasiado grande")){"read"}else{"transport"},
                };
                checks.push(serde_json::json!({"id":item.id,"native":native,"category":category}));
            }
        }
        println!("{}",serde_json::json!({"result":if checks.len()==catalog.designs.len()*2 {"complete"}else{"time_budget_reached"},"catalog_count":catalog.designs.len(),"checks":checks}));
    }

    #[test] fn original_geometry_and_distinct_thumbnail_are_bounded_and_legacy_cannot_be_used() {
        assert!(native_geometry(736,460));assert!(native_geometry(1600,1073));
        for dimensions in [(320,200),(639,460),(736,399),(4097,969)] {assert!(!native_geometry(dimensions.0,dimensions.1));}
        let (legacy,_)=fixture();
        let session=crate::cloud_sync::Session {server:"https://example.invalid".into(),user_id:"fixture".into(),access_token:"fixture".into()};
        assert!(download_purchased(&session,&legacy).unwrap_err().to_string().contains("no se ampliará"));
        let mut value:serde_json::Value=serde_json::from_slice(include_bytes!("../../server/design-catalog.json")).unwrap();value["entitled"]=serde_json::json!(true);
        let legacy_catalog:serde_json::Value=serde_json::from_slice(include_bytes!("../../server/design-catalog.legacy.json")).unwrap();
        let thumbnail=legacy_catalog["designs"][0].clone();value["designs"][0]["width"]=736.into();value["designs"][0]["height"]=460.into();value["designs"][0]["thumbnail"]=thumbnail;
        let parsed=parse_purchased_catalog(&serde_json::to_vec(&value).unwrap()).unwrap();
        assert_eq!(parsed.designs[0].width,736);assert_eq!(parsed.designs[0].thumbnail.as_ref().unwrap().width,320);
        value["designs"][0]["thumbnail"]["id"]="ws-002".into();
        assert!(parse_purchased_catalog(&serde_json::to_vec(&value).unwrap()).is_err());
    }
    #[test] fn expanded_catalog_accepts_legacy_and_new_counts_without_truncation_and_rejects_invalid_sequences() {
        let mut value:serde_json::Value=serde_json::from_slice(include_bytes!("../../server/design-catalog.json")).unwrap();
        value["entitled"]=serde_json::json!(true);
        let expanded=parse_purchased_catalog(&serde_json::to_vec(&value).unwrap()).unwrap();
        assert_eq!(expanded.designs.len(),56);assert_eq!(expanded.designs[50].id,"ws-051");assert_eq!(expanded.designs[55].id,"ws-056");
        let originals=value["designs"].as_array().unwrap().clone();
        for count in [50,56,MAX_INCLUDED_DESIGNS] {
            let items:Vec<_>=(1..=count).map(|index| {let mut item=originals[0].clone();item["id"]=serde_json::json!(format!("ws-{index:03}"));item["filename"]=serde_json::json!(format!("ws-{index:03}.png"));item}).collect();
            value["designs"]=serde_json::json!(items);
            assert_eq!(parse_purchased_catalog(&serde_json::to_vec(&value).unwrap()).unwrap().designs.len(),count);
        }
        for count in [49,MAX_INCLUDED_DESIGNS+1] {
            let items:Vec<_>=(1..=count).map(|index| {let mut item=originals[0].clone();item["id"]=serde_json::json!(format!("ws-{index:03}"));item["filename"]=serde_json::json!(format!("ws-{index:03}.png"));item}).collect();
            value["designs"]=serde_json::json!(items);assert!(parse_purchased_catalog(&serde_json::to_vec(&value).unwrap()).is_err());
        }
        value["designs"]=serde_json::json!(originals);value["designs"][51]=value["designs"][50].clone();
        assert!(parse_purchased_catalog(&serde_json::to_vec(&value).unwrap()).is_err());
        value["entitled"]=serde_json::json!(false);value["designs"]=serde_json::json!([]);
        assert!(parse_purchased_catalog(&serde_json::to_vec(&value).unwrap()).unwrap().designs.is_empty());
    }
    fn fixture()->(PurchasedDesign,Vec<u8>) {
        let mut bytes=Vec::new();image::DynamicImage::new_rgb8(320,200).write_to(&mut Cursor::new(&mut bytes),image::ImageFormat::Png).unwrap();
        (PurchasedDesign {id:"ws-001".into(),title:"Fixture".into(),filename:"ws-001.png".into(),sha256:crate::bank_backup::sha256(&bytes).unwrap(),bytes:bytes.len(),width:320,height:200,thumbnail:None},bytes)
    }
    fn native_fixture()->(PurchasedDesign,Vec<u8>) {
        let mut bytes=Vec::new();image::DynamicImage::new_rgb8(NATIVE_DESIGN_SIZE.0,NATIVE_DESIGN_SIZE.1).write_to(&mut Cursor::new(&mut bytes),image::ImageFormat::Png).unwrap();
        (PurchasedDesign {id:"ws-051".into(),title:"Nativo".into(),filename:"ws-051.png".into(),sha256:crate::bank_backup::sha256(&bytes).unwrap(),bytes:bytes.len(),width:NATIVE_DESIGN_SIZE.0,height:NATIVE_DESIGN_SIZE.1,thumbnail:None},bytes)
    }
    #[test] fn native_card_assets_keep_their_four_mib_bound_and_capability_while_legacy_stays_one_mib() {
        assert_eq!(NATIVE_CATALOG_CAPABILITY,"native-2k-v2");
        assert_eq!((NATIVE_DESIGN_SIZE.0,NATIVE_DESIGN_SIZE.1),(crate::image_skin::CARD_WIDTH,crate::image_skin::CARD_HEIGHT));
        let (mut item,bytes)=native_fixture();verify_purchased(&item,&bytes).unwrap();
        let mut wrong=item.clone();wrong.width+=1;assert!(verify_purchased(&wrong,&bytes).is_err());
        let mut value:serde_json::Value=serde_json::from_slice(include_bytes!("../../server/design-catalog.json")).unwrap();
        value["entitled"]=serde_json::json!(true);
        let mut native=value["designs"][50].clone();
        native["width"]=serde_json::json!(1_536);native["height"]=serde_json::json!(969);native["bytes"]=serde_json::json!(3_500_000);
        value["designs"][50]=native.clone();
        assert_eq!(parse_purchased_catalog(&serde_json::to_vec(&value).unwrap()).unwrap().designs[50].width,1_536);
        native["bytes"]=serde_json::json!(MAX_ASSET_BYTES+1);value["designs"][50]=native.clone();
        assert!(parse_purchased_catalog(&serde_json::to_vec(&value).unwrap()).is_err());
        native["bytes"]=serde_json::json!(1024);native["width"]=serde_json::json!(4_097);value["designs"][50]=native.clone();
        assert!(parse_purchased_catalog(&serde_json::to_vec(&value).unwrap()).is_err());
        let mut oversized=value["designs"][0].clone();oversized["bytes"]=serde_json::json!(LEGACY_ASSET_BYTES+1);value["designs"][0]=oversized;
        assert!(parse_purchased_catalog(&serde_json::to_vec(&value).unwrap()).is_err());
        item.bytes+=1;assert!(verify_purchased(&item,&bytes).is_err());
    }
    #[test] fn purchased_png_requires_exact_hash_size_format_and_dimensions() {
        let (mut item,bytes)=fixture();verify_purchased(&item,&bytes).unwrap();
        assert!(verify_purchased(&item,b"not a png").is_err());
        item.bytes+=1;assert!(verify_purchased(&item,&bytes).is_err());item.bytes-=1;
        item.sha256="0".repeat(64);assert!(verify_purchased(&item,&bytes).is_err());
        let mut other=Vec::new();image::DynamicImage::new_rgb8(321,200).write_to(&mut Cursor::new(&mut other),image::ImageFormat::Png).unwrap();item.sha256=crate::bank_backup::sha256(&other).unwrap();item.bytes=other.len();assert!(verify_purchased(&item,&other).is_err());
    }
    #[test] fn library_storage_is_separated_by_account_and_service() {
        let a=crate::cloud_sync::Session {server:"https://example.test".into(),access_token:"unused".into(),user_id:"one".into()};
        let mut b=a.clone();b.user_id="two".into();assert_ne!(account_dir(&a).unwrap(),account_dir(&b).unwrap());
        b=a.clone();b.server="https://other.test".into();assert_ne!(account_dir(&a).unwrap(),account_dir(&b).unwrap());
        b.server="http://attacker.test".into();assert!(account_dir(&b).is_err());
    }
    #[test] fn unsafe_cache_hash_is_rejected_before_filesystem_access() {
        let (mut item,_)=fixture();item.sha256="../../foreign".into();
        let session=crate::cloud_sync::Session {server:"https://example.test".into(),access_token:"unused".into(),user_id:"one".into()};
        assert!(cached_purchased(&session,&item).is_err());
    }
    #[test] fn damaged_cache_is_repaired_atomically_and_invalid_download_preserves_existing() {
        let (item,bytes)=fixture();
        let root=std::env::temp_dir().join(format!("ws-cache-repair-{}",crate::flasher::generate_token()));
        std::fs::create_dir_all(&root).unwrap();let path=root.join("preview.png");
        std::fs::write(&path,b"interrupted old download").unwrap();
        assert!(store_purchased_cache(&path,&item,b"invalid network response").is_err());
        assert_eq!(std::fs::read(&path).unwrap(),b"interrupted old download");
        store_purchased_cache(&path,&item,&bytes).unwrap();verify_purchased(&item,&read_image(&path).unwrap()).unwrap();
        std::thread::scope(|scope| {for _ in 0..4 {let path=&path;let item=&item;let bytes=&bytes;scope.spawn(move|| {store_purchased_cache(path,item,bytes).unwrap();});}});
        assert_eq!(std::fs::read_dir(&root).unwrap().count(),1);
        assert_eq!(read_image(&path).unwrap(),bytes);std::fs::remove_dir_all(root).unwrap();
    }
}
