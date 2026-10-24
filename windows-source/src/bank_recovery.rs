//! Explicit original recovery ledger. All evidence is immutable and private;
//! neither a ledger nor a cloud proof alone declares the phone restored.
use anyhow::{Context,Result};
use serde::{Deserialize,Serialize};
use std::{collections::BTreeMap,fs,path::{Path,PathBuf}};

#[derive(Clone,Serialize,Deserialize,PartialEq,Eq)] #[serde(deny_unknown_fields)]
struct Record {size:usize,sha256:String}
#[derive(Clone,Serialize,Deserialize,PartialEq,Eq)] #[serde(deny_unknown_fields)]
struct Ledger {
    version:u32,owner:String,udid:String,card_hash:String,baseline:String,
    local_operation:String,remote_operation:String,inventory:String,archives:Vec<String>,
    files:BTreeMap<String,Record>,local_files:Vec<String>,
}
pub(crate) struct RecoveryPlan {
    root:PathBuf,directory:PathBuf,ledger:Ledger,proof:crate::cloud_sync::VerifiedRemoteRecovery,
}
fn digest(bytes:&[u8])->Result<String> {crate::bank_backup::sha256(bytes)}
fn baseline_digest(value:&crate::bank_backup::Baseline)->Result<String> {digest(&serde_json::to_vec(value)?)}
fn persist(path:&Path,bytes:&[u8])->Result<()> {
    crate::storage::safe_chain(path)?;
    if path.exists() {anyhow::ensure!(fs::read(path)?==bytes,"recovery ledger evidence conflict");}
    else {crate::storage::write_new(path,bytes)?;}
    anyhow::ensure!(fs::read(path)?==bytes,"recovery ledger failed readback");Ok(())
}
fn safe_operation(value:&str)->bool {value.len()>=8 && value.len()<=128 && value.bytes().all(|b|b.is_ascii_alphanumeric() || b==b'-' || b==b'_')}
fn safe_evidence(value:&str)->bool {
    let Some((operation,leaf))=value.split_once('/') else{return false;};
    safe_operation(operation) && !leaf.contains(['/', '\\', ':']) && (crate::wallet_backup::CARD_ARTWORK_ASSETS.contains(&leaf)
        || regex::Regex::new(r"^state-[0-9]{4,}-(prepared|applied|originals_verified|restored)\.json$").unwrap().is_match(leaf))
}
fn read_ledger(directory:&Path)->Result<Ledger> {
    let path=directory.join("plan.json");crate::storage::safe_chain(&path)?;
    let meta=fs::symlink_metadata(&path)?;anyhow::ensure!(meta.is_file() && meta.len()<=1024*1024,"unsafe recovery ledger");
    let ledger:Ledger=serde_json::from_slice(&fs::read(&path)?)?;
    anyhow::ensure!(ledger.version==1 && crate::wallet_export::is_exact_card_hash(&ledger.card_hash)
        && safe_operation(&ledger.local_operation) && safe_operation(&ledger.remote_operation)
        && !ledger.files.is_empty() && ledger.files.len()<=4096,"invalid recovery ledger");
    for (name,record) in &ledger.files {
        anyhow::ensure!(safe_evidence(name) && record.size<=32*1024*1024,"invalid recovery evidence binding");
        let path=directory.join("evidence").join(name);crate::storage::safe_chain(&path)?;
        let meta=fs::symlink_metadata(&path)?;anyhow::ensure!(meta.is_file() && !meta.file_type().is_symlink() && meta.len()==record.size as u64,"recovery evidence missing or unsafe");
        anyhow::ensure!(digest(&fs::read(path)?)?==record.sha256,"recovery evidence integrity mismatch");
    }
    anyhow::ensure!(directory.file_name().and_then(|n|n.to_str())==Some(digest(&serde_json::to_vec(&ledger)?)?.as_str()),"recovery ledger directory mismatch");
    Ok(ledger)
}
fn completed_at(directory:&Path,ledger:&Ledger)->Result<bool> {
    let path=directory.join("completed.json");crate::storage::safe_chain(&path)?;
    let meta=match fs::symlink_metadata(&path) {Err(error) if error.kind()==std::io::ErrorKind::NotFound=>return Ok(false),result=>result?};
    anyhow::ensure!(meta.is_file() && !meta.file_type().is_symlink() && meta.len()<=4096,"unsafe recovery completion record");
    #[derive(Deserialize)] #[serde(deny_unknown_fields)] struct Complete {version:u32,plan:String,originals_verified:bool}
    let complete:Complete=serde_json::from_slice(&fs::read(path)?)?;
    anyhow::ensure!(complete.version==1 && complete.originals_verified && complete.plan==digest(&serde_json::to_vec(ledger)?)?,"unverified recovery completion record");Ok(true)
}
pub(crate) fn has_pending_exact(udid:&str,hash:&str)->Result<bool> {
    let root=crate::storage::root();
    let Some((_,ledger))=pending_at(&root,udid)? else{return Ok(false);};
    if ledger.card_hash!=hash {return Ok(false);}
    let mut identity=udid.as_bytes().to_vec();identity.push(0);identity.extend_from_slice(hash.as_bytes());
    let baseline=crate::bank_backup::validate_bundle_directory(&root.join("bank-backups").join(digest(&identity)?))?;
    Ok(baseline_digest(&baseline)?==ledger.baseline)
}
fn pending_at(root:&Path,udid:&str)->Result<Option<(PathBuf,Ledger)>> {
    let ledgers=root.join("bank-recovery-ledgers");crate::storage::safe_chain(&ledgers)?;
    if !ledgers.exists() {return Ok(None);}
    let mut found=None;let mut count=0;
    for entry in fs::read_dir(&ledgers)? {
        count+=1;anyhow::ensure!(count<=1000,"too many recovery ledgers");let path=entry?.path();
        crate::storage::safe_chain(&path)?;
        // A crash before plan.json commits has not authorized any movement.
        if !path.join("plan.json").exists() {continue;}
        let ledger=read_ledger(&path)?;
        if ledger.udid==udid && !completed_at(&path,&ledger)? {
            anyhow::ensure!(found.is_none(),"multiple pending recovery plans for this device");found=Some((path,ledger));
        }
    }Ok(found)
}
fn evidence_for(root:&Path,proof:&crate::cloud_sync::VerifiedRemoteRecovery,local_operation:&str)->Result<BTreeMap<String,Vec<u8>>> {
    let baseline=proof.baseline();
    let mut files=BTreeMap::new();
    for (name,bytes) in crate::bank_changes::recovery_evidence_at(&root.join("bank-change-intents"),local_operation,&baseline.udid,&baseline.card_hash)? {
        files.insert(format!("{local_operation}/{name}"),bytes);
    }
    let union=proof.active().operation_dir.parent().context("remote operation union missing")?;
    for path in crate::bank_changes::cloud_operation_dirs(union,&baseline.udid,&baseline.card_hash)? {
        let operation=path.file_name().and_then(|n|n.to_str()).context("invalid remote operation")?;
        for (name,bytes) in crate::bank_changes::recovery_evidence_at(union,operation,&baseline.udid,&baseline.card_hash)? {
            let name=format!("{operation}/{name}");
            if let Some(previous)=files.insert(name,bytes.clone()) {anyhow::ensure!(previous==bytes,"local and remote operation evidence differs");}
        }
    }Ok(files)
}
impl RecoveryPlan {
    pub(crate) fn root(&self)->&Path {&self.root}
    pub(crate) fn baseline(&self)->&crate::bank_backup::Baseline {self.proof.baseline()}
    pub(crate) fn id(&self)->Result<String> {digest(&serde_json::to_vec(&self.ledger)?)}
    pub(crate) fn revalidate(&self)->Result<()> {
        let root=&self.root;
        anyhow::ensure!(read_ledger(&self.directory)?==self.ledger,"recovery ledger changed");
        self.proof.revalidate_owner_at(root)?;
        anyhow::ensure!(self.proof.owner_digest()?==self.ledger.owner && self.proof.active().operation_dir.file_name().and_then(|n|n.to_str())==Some(self.ledger.remote_operation.as_str())
            && self.ledger.archives.iter().all(|id|self.proof.archive_ids().contains(id)),"fresh cloud proof differs from recovery plan");
        let mut identity=self.ledger.udid.as_bytes().to_vec();identity.push(0);identity.extend_from_slice(self.ledger.card_hash.as_bytes());
        let baseline=crate::bank_backup::validate_bundle_directory(&root.join("bank-backups").join(digest(&identity)?))?;
        anyhow::ensure!(baseline==*self.proof.baseline() && baseline_digest(&baseline)?==self.ledger.baseline,"recovery original changed");
        let local=crate::bank_changes::recovery_evidence_at(&root.join("bank-change-intents"),&self.ledger.local_operation,&self.ledger.udid,&self.ledger.card_hash)?;
        let union=self.proof.active().operation_dir.parent().context("remote recovery union missing")?;
        let mut remote=BTreeMap::new();
        for path in crate::bank_changes::cloud_operation_dirs(union,&self.ledger.udid,&self.ledger.card_hash)? {
            let operation=path.file_name().and_then(|n|n.to_str()).context("invalid recovery operation")?;
            for (name,bytes) in crate::bank_changes::recovery_evidence_at(union,operation,&self.ledger.udid,&self.ledger.card_hash)? {remote.insert(format!("{operation}/{name}"),bytes);}
        }
        for (name,record) in &self.ledger.files {
            let (_,leaf)=name.split_once('/').unwrap();
            let bytes=if self.ledger.local_files.contains(name) {local.get(leaf)} else {remote.get(name)}.context("recorded recovery source missing")?;
            anyhow::ensure!(bytes.len()==record.size && digest(bytes)?==record.sha256,"recorded recovery source changed");
        }
        Ok(())
    }
    pub(crate) fn owns(&self,udid:&str,hash:&str,leaf:&str,bytes:&[u8])->Result<bool> {
        self.revalidate()?;
        anyhow::ensure!(udid==self.ledger.udid && hash==self.ledger.card_hash && crate::wallet_backup::CARD_ARTWORK_ASSETS.contains(&leaf),"recovery target differs from plan");
        let name=format!("{}/{leaf}",self.ledger.local_operation);
        let local_owned=self.ledger.local_files.contains(&name) && self.ledger.files.get(&name).is_some_and(|record|record.size==bytes.len() && digest(bytes).is_ok_and(|sha|sha==record.sha256));
        Ok(local_owned || self.proof.active().assets.get(leaf).is_some_and(|owned|owned==bytes))
    }
    // Opaque proof can only be minted by flasher after fresh all-originals and
    // cache readback. No phase is invented by a downloaded archive or ledger.
    pub(crate) fn finish(&self,physical:crate::flasher::VerifiedOriginalRestoration)->Result<()> {
        self.finish_with_checkpoint(physical,|_|Ok(()))
    }
    fn finish_with_checkpoint(&self,physical:crate::flasher::VerifiedOriginalRestoration,mut checkpoint:impl FnMut(&str)->Result<()>)->Result<()> {
        let _nested=crate::storage::operation_lease_for_import(&self.root)?;
        self.revalidate()?;anyhow::ensure!(physical.matches(self),"physical recovery proof is for another plan");
        let root=&self.root;let destination=root.join("bank-change-intents");
        let operations=self.ledger.files.keys().map(|name|name.split_once('/').unwrap().0.to_string()).collect::<std::collections::BTreeSet<_>>();
        let source=self.directory.join("evidence");let mut copied=BTreeMap::new();
        for operation in &operations {
            let files=crate::bank_changes::recovery_evidence_at(&source,operation,&self.ledger.udid,&self.ledger.card_hash)?;
            crate::bank_changes::validate_recovery_backfill_at(&destination,operation,&self.ledger.udid,&self.ledger.card_hash,&files)?;
            copied.insert(operation.clone(),files);
        }
        checkpoint("before_backfill")?;
        crate::storage::safe_chain(&destination)?;fs::create_dir_all(&destination)?;
        for (operation,files) in copied {
            let target=destination.join(&operation);
            if target.exists() {
                for (name,bytes) in files {persist(&target.join(name),&bytes)?;}
            }else {
                // No asset-only or partially copied operation becomes visible
                // to the normal history scanner, even after process death.
                let stage=self.directory.join("backfill").join(&operation);
                for (name,bytes) in files {persist(&stage.join(name),&bytes)?;}
                crate::bank_changes::recovery_evidence_at(stage.parent().unwrap(),&operation,&self.ledger.udid,&self.ledger.card_hash)?;
                crate::storage::safe_chain(&target)?;fs::rename(&stage,&target)?;
                crate::bank_changes::recovery_evidence_at(&destination,&operation,&self.ledger.udid,&self.ledger.card_hash)?;
                checkpoint("after_publish_operation")?;
            }
        }
        for operation in operations {crate::bank_changes::complete_exact_recovery_operation(&operation,&self.ledger.udid,&self.ledger.card_hash,&physical,self)?;checkpoint("after_complete_operation")?;}
        checkpoint("before_completion_receipt")?;
        persist(&self.directory.join("completed.json"),&serde_json::to_vec(&serde_json::json!({"version":1,"plan":self.id()?,"originals_verified":true}))?)?;
        Ok(())
    }
}
pub(crate) fn prepare_for_device(session:&crate::cloud_sync::Session,udid:&str,progress:impl FnMut(&'static str))->Result<RecoveryPlan> {
    prepare_selected(session,udid,None,progress)
}
pub(crate) fn prepare_selected(session:&crate::cloud_sync::Session,udid:&str,selected:Option<&str>,mut progress:impl FnMut(&'static str))->Result<RecoveryPlan> {
    let _lease=crate::storage::operation_lease()?;let root=crate::storage::root();
    if let Some((directory,ledger))=pending_at(&root,udid)? {
        anyhow::ensure!(selected.is_none_or(|hash|hash==ledger.card_hash),"another card recovery already owns this device");
        let proof=crate::cloud_sync::prepare_remote_recovery(session,udid,&ledger.card_hash,&mut progress)?;
        let plan=RecoveryPlan {root:root.to_path_buf(),directory,ledger,proof};plan.revalidate()?;return Ok(plan);
    }
    let mut interrupted=crate::bank_changes::interrupted_restorations_for_device(udid)?.into_iter().filter(|intent|selected.is_none_or(|hash|intent.card_hash==hash)).collect::<Vec<_>>();
    anyhow::ensure!(interrupted.len()==1,"recovery requires one exact interrupted local operation");
    let local=interrupted.remove(0);let proof=crate::cloud_sync::prepare_remote_recovery(session,udid,&local.card_hash,&mut progress)?;
    new_plan_at(&root,local,proof)
}
fn new_plan_at(root:&Path,local:crate::bank_changes::Intent,proof:crate::cloud_sync::VerifiedRemoteRecovery)->Result<RecoveryPlan> {
    let udid=local.udid.as_str();
    let local_operation=local.operation_dir.file_name().and_then(|n|n.to_str()).context("invalid local recovery operation")?.to_string();
    let remote_operation=proof.active().operation_dir.file_name().and_then(|n|n.to_str()).context("invalid remote recovery operation")?.to_string();
    let local_files=crate::bank_changes::recovery_evidence_at(&root.join("bank-change-intents"),&local_operation,udid,&local.card_hash)?.keys().map(|name|format!("{local_operation}/{name}")).collect();
    let evidence=evidence_for(root,&proof,&local_operation)?;let mut total=0usize;
    let files=evidence.iter().map(|(name,bytes)| {total=total.checked_add(bytes.len()).context("recovery ledger overflow")?;
        anyhow::ensure!(total<=512*1024*1024,"recovery ledger too large");Ok((name.clone(),Record {size:bytes.len(),sha256:digest(bytes)?}))}).collect::<Result<BTreeMap<_,_>>>()?;
    anyhow::ensure!(files.len()<=4096,"recovery ledger has too many files");
    let ledger=Ledger {version:1,owner:proof.owner_digest()?,udid:udid.into(),card_hash:local.card_hash,baseline:baseline_digest(proof.baseline())?,local_operation,remote_operation,inventory:proof.inventory_digest().into(),archives:proof.archive_ids().to_vec(),files,local_files};
    let bytes=serde_json::to_vec(&ledger)?;anyhow::ensure!(bytes.len()<=1024*1024,"recovery ledger metadata too large");
    let directory=root.join("bank-recovery-ledgers").join(digest(&bytes)?);
    for (name,bytes) in evidence {persist(&directory.join("evidence").join(name),&bytes)?;}
    persist(&directory.join("plan.json"),&bytes)?;
    let plan=RecoveryPlan {root:root.to_path_buf(),directory,ledger,proof};plan.revalidate()?;Ok(plan)
}

#[cfg(test)] mod tests {
    use super::*;
    fn fixture()->(PathBuf,PathBuf,RecoveryPlan) {
        let source=std::env::temp_dir().join(format!("ws-recovery-source-{}",crate::flasher::generate_token()));
        let target=std::env::temp_dir().join(format!("ws-recovery-target-{}",crate::flasher::generate_token()));
        let baseline=crate::cloud_sync::tests::fixture_baseline();
        let pairs=|pdf:&[u8]|crate::wallet_backup::CARD_ARTWORK_ASSETS.iter().map(|leaf|(leaf.to_string(),if leaf.ends_with(".pdf") {pdf.to_vec()} else {baseline.remote_artwork_bytes.clone()})).collect::<Vec<_>>();
        let remote_pairs=pairs(b"%PDF-1.7\nremote design\n%%EOF");
        let bank=crate::bank_backup::save_fixture_at(&source.join("bank-backups"),&baseline).unwrap();
        crate::bank_changes::save_fixture_applied_at(&source.join("bank-change-intents"),&baseline.udid,&baseline.card_hash,&remote_pairs).unwrap();
        let bundle=crate::cloud_sync::export_bundle(&source,&bank).unwrap();
        crate::bank_backup::save_fixture_at(&target.join("bank-backups"),&baseline).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(2));
        let local_pairs=pairs(b"%PDF-1.7\nlocal design\n%%EOF");
        let operation=crate::bank_changes::save_fixture_prepared_at(&target.join("bank-change-intents"),&baseline.udid,&baseline.card_hash,&local_pairs).unwrap();
        let local=crate::bank_changes::Intent {operation_dir:operation,udid:baseline.udid.clone(),card_hash:baseline.card_hash.clone(),assets:local_pairs.into_iter().collect(),absent_assets:Vec::new(),phase:"prepared".into()};
        let proof=crate::cloud_sync::tests::proof_fixture(&target,&baseline,&[bundle]).unwrap();
        let plan=new_plan_at(&target,local,proof).unwrap();(source,target,plan)
    }
    #[test] fn recovery_ledger_binds_exact_local_or_remote_bytes_and_preserves_unknown() {
        let (source,target,plan)=fixture();let baseline=plan.baseline();let leaf="cardBackgroundCombined.pdf";
        assert!(plan.owns(&baseline.udid,&baseline.card_hash,leaf,b"%PDF-1.7\nlocal design\n%%EOF").unwrap());
        assert!(plan.owns(&baseline.udid,&baseline.card_hash,leaf,b"%PDF-1.7\nremote design\n%%EOF").unwrap());
        assert!(!plan.owns(&baseline.udid,&baseline.card_hash,leaf,b"%PDF-1.7\nUNKNOWN\n%%EOF").unwrap());
        assert!(plan.owns("synthetic-other-device",&baseline.card_hash,leaf,b"%PDF-1.7\nremote design\n%%EOF").is_err());
        assert!(pending_at(&target,&baseline.udid).unwrap().is_some());
        assert!(!target.join("bank-change-intents").join(&plan.ledger.remote_operation).exists());
        let path=plan.directory.join("evidence").join(format!("{}/{leaf}",plan.ledger.remote_operation));
        fs::write(&path,b"tampered retained evidence").unwrap();assert!(plan.revalidate().is_err());
        fs::remove_dir_all(source).unwrap();fs::remove_dir_all(target).unwrap();
    }
    #[test] fn physical_recovery_capability_holds_exclusive_lease_until_consumed_or_dropped() {
        let (source,target,plan)=fixture();
        let token=crate::flasher::VerifiedOriginalRestoration::synthetic_completed(&plan);
        let other=target.clone();assert!(std::thread::spawn(move||crate::storage::operation_lease_for_import(&other).is_err()).join().unwrap());
        drop(token);
        let other=target.clone();assert!(std::thread::spawn(move||crate::storage::operation_lease_for_import(&other).is_ok()).join().unwrap());
        fs::remove_dir_all(source).unwrap();fs::remove_dir_all(target).unwrap();
    }
    #[test] fn recovery_ledger_crash_boundaries_resume_without_latest_intent_selection() {
        let (source,target,plan)=fixture();let udid=plan.ledger.udid.clone();let hash=plan.ledger.card_hash.clone();
        for stop in ["before_backfill","after_publish_operation","after_complete_operation","before_completion_receipt"] {
            let result=plan.finish_with_checkpoint(crate::flasher::VerifiedOriginalRestoration::synthetic_completed(&plan),|stage| {
                anyhow::ensure!(stage!=stop,"synthetic interruption");Ok(())
            });assert!(result.is_err(),"{stop}");
            assert!(pending_at(&target,&udid).unwrap().is_some(),"{stop}");plan.revalidate().unwrap();
            // Every visible managed directory has a valid full operation even
            // where the ordinary combined latest selector must reject it.
            for entry in fs::read_dir(target.join("bank-change-intents")).unwrap() {
                let path=entry.unwrap().path();let operation=path.file_name().unwrap().to_str().unwrap();
                crate::bank_changes::recovery_evidence_at(&target.join("bank-change-intents"),operation,&udid,&hash).unwrap();
            }
        }
        plan.finish(crate::flasher::VerifiedOriginalRestoration::synthetic_completed(&plan)).unwrap();
        assert!(pending_at(&target,&udid).unwrap().is_none());
        crate::bank_changes::validate_cloud_chain(&target.join("bank-change-intents"),&udid,&hash).unwrap();
        fs::write(plan.directory.join("completed.json"),b"{").unwrap();assert!(pending_at(&target,&udid).is_err(),"torn completion record is not success");
        fs::remove_dir_all(source).unwrap();fs::remove_dir_all(target).unwrap();
    }
}
