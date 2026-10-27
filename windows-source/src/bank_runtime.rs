//! Device proof orchestration for an exact bank-card baseline.
use anyhow::{Context, Result};
use std::collections::BTreeMap;
use crate::device::ConnectionMode;
use crate::bank_backup::{Baseline, LocalAssetState};
use crate::wallet_export::{BANK_MANIFEST, MetadataState, capture_verified_original_asset, inspect_bank_overrides};

fn prove_local_originals<F>(declarations:&[crate::bank_artwork::DeclaredArtwork],originals:&BTreeMap<String,LocalAssetState>,mut download:F)->Result<()>
where F:FnMut(&crate::bank_artwork::DeclaredArtwork)->Result<Vec<u8>> {
    anyhow::ensure!(originals.len()==crate::wallet_backup::CARD_ARTWORK_ASSETS.len(),"Falta procedencia de un recurso local");
    for leaf in crate::wallet_backup::CARD_ARTWORK_ASSETS {
        match originals.get(leaf).context("Falta un estado original exacto")? {
            LocalAssetState::Absent=>{},
            LocalAssetState::Present {bytes}=> {
                let declaration=declarations.iter().find(|entry|entry.name==leaf).context("Un recurso presente no tiene declaración del emisor; no se certificará el diseño actual como original")?;
                let issuer=download(declaration)?;
                anyhow::ensure!(issuer.len()==declaration.size && crate::bank_backup::sha1(&issuer)?==declaration.sha1.to_ascii_lowercase(),"No se verificó el recurso exacto del emisor");
                anyhow::ensure!(bytes==&issuer,"El recurso local es distinto al emisor; conserva la evidencia y recupera el original previo");
            }
        }
    } Ok(())
}

pub fn capture_baseline<L>(udid: &str, mode: ConnectionMode, hash: &str, mut log: L) -> Result<std::path::PathBuf>
where L: FnMut(&str) {
    let _lease=crate::storage::operation_lease()?;
    crate::storage::ensure_card_ready(udid,hash)?;
    if crate::bank_backup::load_baseline(udid, hash)?.is_some() {
        log("Estado original bancario ya guardado: integridad verificada.");
        return crate::bank_backup::baseline_path(udid, hash)?.context("Verified bank baseline disappeared");
    }
    crate::device::ensure_wallet_version_supported(udid)?;
    let manifest = capture_verified_original_asset(udid, mode, hash, BANK_MANIFEST, &mut log)?;
    let declarations = crate::bank_artwork::parse_manifest(&manifest)?;
    let states = inspect_bank_overrides(udid, mode, hash, &manifest, &mut log)?;
    let mut originals = BTreeMap::new();
    for leaf in crate::wallet_backup::CARD_ARTWORK_ASSETS {
        let original = match states.get(leaf).context("Falta un estado local comprobado")? {
            MetadataState::Absent => LocalAssetState::Absent,
            MetadataState::Present { .. } | MetadataState::Unreadable => {
                anyhow::ensure!(declarations.iter().any(|entry|entry.name==leaf),
                    "No hay declaración del emisor para este recurso presente o incierto; no se capturará como original");
                // A denial is never absence. Only a successful export, return
                // and equal fresh readback can turn it into a present baseline.
                let bytes = capture_verified_original_asset(udid, mode, hash, leaf, &mut log)?;
                LocalAssetState::Present { bytes }
            }
        };
        originals.insert(leaf.into(), original);
    }
    prove_local_originals(&declarations,&originals,crate::bank_artwork::download_verified_artwork)?;
    let declaration = declarations.first().context("Sin imagen bancaria verificable")?;
    let remote = crate::bank_artwork::download_verified_artwork(declaration)?;
    let baseline = Baseline { udid: udid.into(), card_hash: hash.into(), manifest_bytes: manifest,
        artwork_name: declaration.name.clone(), remote_size: declaration.size,
        remote_sha1: declaration.sha1.clone(), remote_artwork_bytes: remote,
        local_assets: originals };
    let path = crate::bank_backup::save_baseline(&baseline)?;
    anyhow::ensure!(crate::bank_backup::load_baseline(udid, hash)?.as_ref() == Some(&baseline),
        "La copia bancaria no pasó la lectura posterior");
    log(&format!("BANK_BASELINE_OK: estado original completo guardado en {}", path.display()));
    Ok(path)
}

#[cfg(test)] mod provenance_tests {
    use super::*;
    fn originals(bytes:&[u8])->BTreeMap<String,LocalAssetState> {
        crate::wallet_backup::CARD_ARTWORK_ASSETS.into_iter().map(|leaf|(leaf.into(),
            if leaf=="cardBackgroundCombined@2x.png" {LocalAssetState::Present {bytes:bytes.to_vec()}} else {LocalAssetState::Absent})).collect()
    }
    fn declaration(bytes:&[u8])->crate::bank_artwork::DeclaredArtwork {
        crate::bank_artwork::DeclaredArtwork {name:"cardBackgroundCombined@2x.png".into(),url:"https://example.apple.com/art.png".into(),size:bytes.len(),sha1:crate::bank_backup::sha1(bytes).unwrap()}
    }
    #[test] fn exact_declared_present_original_is_allowed_but_custom_is_denied() {
        let issuer=b"owned issuer fixture"; let declared=vec![declaration(issuer)];
        assert!(prove_local_originals(&declared,&originals(issuer),|_|Ok(issuer.to_vec())).is_ok());
        assert!(prove_local_originals(&declared,&originals(b"custom fixture"),|_|Ok(issuer.to_vec())).is_err());
    }
    #[test] fn undeclared_present_or_wrong_leaf_never_certifies_original() {
        assert!(prove_local_originals(&[],&originals(b"owned fixture"),|_|panic!("undeclared resource must not download")).is_err());
        let mut wrong=declaration(b"owned fixture"); wrong.name="cardBackgroundCombined@3x.png".into();
        assert!(prove_local_originals(&[wrong],&originals(b"owned fixture"),|_|panic!("other leaf is not provenance")).is_err());
    }
    #[test] fn download_checksum_failure_and_missing_slot_fail_closed() {
        let bytes=b"owned fixture"; let declared=vec![declaration(bytes)];
        assert!(prove_local_originals(&declared,&originals(bytes),|_|Ok(b"different issuer".to_vec())).is_err());
        let mut missing=originals(bytes); missing.remove("cardBackgroundCombined.pdf");
        assert!(prove_local_originals(&declared,&missing,|_|Ok(bytes.to_vec())).is_err());
    }
}
