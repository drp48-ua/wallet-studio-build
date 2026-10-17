//! Local, bounded image inputs. The library never interprets URLs or manifests.
use anyhow::{Context, Result};
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};

pub const MAX_IMAGE_BYTES: u64 = 32 * 1024 * 1024;
const MAX_PIXELS: u64 = 16 * 1024 * 1024;
pub const MAX_DESIGNS: usize = 100;
pub const MAX_INCLUDED_DESIGNS: usize = 100;

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
pub struct PurchasedDesign { pub id:String,pub title:String,pub filename:String,pub sha256:String,pub bytes:usize,pub width:u32,pub height:u32 }
#[derive(serde::Deserialize)]
pub struct PurchasedCatalog { pub schema:u32,pub pack_id:String,pub entitled:bool,pub designs:Vec<PurchasedDesign> }
pub fn account_dir(session:&crate::cloud_sync::Session)->Result<PathBuf> {
    anyhow::ensure!(crate::cloud_sync::valid_server(&session.server) && !session.user_id.is_empty(),"Cuenta no válida");
    let key=crate::bank_backup::sha256(format!("{}|{}",session.server,session.user_id).as_bytes())?;
    let root=crate::storage::root().join("design-library").join(key);crate::storage::safe_chain(&root)?;Ok(root)
}
pub fn purchased_catalog(session:&crate::cloud_sync::Session)->Result<PurchasedCatalog> {
    anyhow::ensure!(crate::cloud_sync::valid_server(&session.server),"Servicio no válido");
    let bytes=crate::auto_update::http_request("GET",&format!("{}/v1/designs",session.server),&[("Authorization".into(),format!("Bearer {}",session.access_token)),("X-Wallet-Studio-Catalog".into(),"expanded-v1".into())],&[],65536)
        .context("No se pudo actualizar el pack: revisa tu conexión o actualiza el servidor")?;
    parse_purchased_catalog(&bytes)
}
fn parse_purchased_catalog(bytes:&[u8])->Result<PurchasedCatalog> {
    anyhow::ensure!(bytes.len()<=65536,"Catálogo demasiado grande");
    let catalog:PurchasedCatalog=serde_json::from_slice(&bytes)?;
    anyhow::ensure!(catalog.schema==1 && catalog.pack_id=="studio-50-v1" && ((!catalog.entitled && catalog.designs.is_empty()) || (catalog.entitled && (50..=MAX_INCLUDED_DESIGNS).contains(&catalog.designs.len()))),"Catálogo no compatible");
    for (index,item) in catalog.designs.iter().enumerate() {
        anyhow::ensure!(item.id==format!("ws-{:03}",index+1) && item.filename==format!("{}.png",item.id) && item.sha256.len()==64 && item.sha256.bytes().all(|b|b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) && item.bytes>0 && item.bytes<=1024*1024 && (item.width,item.height)==(320,200) && !item.title.is_empty() && item.title.len()<=320 && !item.title.chars().any(char::is_control),"Metadatos de diseño no válidos");
    } Ok(catalog)
}
pub fn verify_purchased(item:&PurchasedDesign,bytes:&[u8])->Result<()> {
    anyhow::ensure!(bytes.len()==item.bytes && bytes.len()<=1024*1024 && bytes.starts_with(b"\x89PNG\r\n\x1a\n") && crate::bank_backup::sha256(bytes)?==item.sha256,"El diseño descargado no pasó la verificación");
    let image=decode_still(bytes)?;anyhow::ensure!((image.width(),image.height())==(320,200),"Tamaño de diseño no válido");Ok(())
}
pub fn download_purchased(session:&crate::cloud_sync::Session,item:&PurchasedDesign)->Result<PathBuf> {
    anyhow::ensure!(crate::cloud_sync::valid_server(&session.server) && item.id.len()==6 && item.id.starts_with("ws-") && item.id[3..].bytes().all(|b|b.is_ascii_digit()),"Diseño no válido");
    // Always ask the service when using a paid design, including cached designs.
    let bytes=crate::auto_update::http_request("GET",&format!("{}/v1/designs/{}",session.server,item.id),&[("Authorization".into(),format!("Bearer {}",session.access_token))],&[],1024*1024)?;
    verify_purchased(item,&bytes)?;
    let path=purchased_cache_path(session,item)?;
    store_purchased_cache(&path,item,&bytes)?;
    Ok(path)
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

#[cfg(test)]
mod paid_tests {
    use super::*;
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
        (PurchasedDesign {id:"ws-001".into(),title:"Fixture".into(),filename:"ws-001.png".into(),sha256:crate::bank_backup::sha256(&bytes).unwrap(),bytes:bytes.len(),width:320,height:200},bytes)
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
