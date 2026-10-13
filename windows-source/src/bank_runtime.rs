//! Device proof orchestration for an exact bank-card baseline.
use anyhow::{Context, Result};
use std::collections::BTreeMap;
use crate::device::ConnectionMode;
use crate::bank_backup::{Baseline, LocalAssetState};
use crate::wallet_export::{BANK_MANIFEST, MetadataState, capture_verified_original_asset, inspect_bank_overrides};

pub fn capture_baseline<L>(udid: &str, mode: ConnectionMode, hash: &str, mut log: L) -> Result<std::path::PathBuf>
where L: FnMut(&str) {
    crate::device::ensure_wallet_version_supported(udid)?;
    if crate::bank_backup::load_baseline(udid, hash)?.is_some() {
        log("Estado original bancario ya guardado: integridad verificada.");
        return crate::bank_backup::baseline_path(udid, hash)?.context("Verified bank baseline disappeared");
    }
    let manifest = capture_verified_original_asset(udid, mode, hash, BANK_MANIFEST, &mut log)?;
    let declarations = crate::bank_artwork::parse_manifest(&manifest)?;
    let states = inspect_bank_overrides(udid, mode, hash, &manifest, &mut log)?;
    let mut originals = BTreeMap::new();
    for leaf in crate::wallet_backup::CARD_ARTWORK_ASSETS {
        let original = match states.get(leaf).context("Falta un estado local comprobado")? {
            MetadataState::Absent => LocalAssetState::Absent,
            MetadataState::Present { .. } | MetadataState::Unreadable => {
                // A denial is never absence. Only a successful export, return
                // and equal fresh readback can turn it into a present baseline.
                let bytes = capture_verified_original_asset(udid, mode, hash, leaf, &mut log)?;
                LocalAssetState::Present { bytes }
            }
        };
        originals.insert(leaf.into(), original);
    }
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
