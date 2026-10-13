//! In-memory presentation snapshots and one-shot, human-selected capture state.
use std::collections::HashMap;

pub struct SnapshotCache<T> { epoch: u64, values: HashMap<String,T> }
impl<T> Default for SnapshotCache<T> {
    fn default() -> Self { Self { epoch: 0, values: HashMap::new() } }
}
impl<T> SnapshotCache<T> {
    pub fn epoch(&self) -> u64 { self.epoch }
    pub fn invalidate(&mut self) { self.epoch = self.epoch.checked_add(1).expect("snapshot epoch overflow"); self.values.clear(); }
    pub fn insert(&mut self, epoch: u64, hash: String, value: T) -> bool {
        if epoch != self.epoch { return false; }
        self.values.insert(hash,value); true
    }
    pub fn get(&self, hash: &str) -> Option<&T> { self.values.get(hash) }
}

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum OriginalState { Missing, Verified, Blocked }
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum CapturePhase { Waiting, Running, Ready, Failed, Blocked }
pub struct AutoCapture { pub hash: String, pub device: Option<String>, pub phase: CapturePhase }
impl AutoCapture {
    pub fn selected(hash: String, device: Option<String>) -> Self { Self { hash, device, phase: CapturePhase::Waiting } }
    pub fn should_start(&mut self, selected_device: Option<&str>, known_device: Option<&str>,
        original: OriginalState, safe: bool) -> bool {
        if self.phase != CapturePhase::Waiting { return false; }
        if let Some(known) = known_device { self.device = Some(known.to_string()); }
        if original == OriginalState::Blocked { self.phase = CapturePhase::Blocked; return false; }
        if original == OriginalState::Verified { self.phase = CapturePhase::Ready; return false; }
        if safe && self.device.as_deref().is_some() && self.device.as_deref() == selected_device {
            self.phase = CapturePhase::Running; return true;
        }
        false
    }
    pub fn finish(&mut self, verified: bool) { self.phase = if verified { CapturePhase::Ready } else { CapturePhase::Failed }; }
    pub fn retry(&mut self) { if self.phase == CapturePhase::Failed { self.phase = CapturePhase::Waiting; } }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn automatic_capture_is_bound_one_shot_and_never_retries_failure_or_corrupt_baseline() {
        let mut capture = AutoCapture::selected("card".into(),Some("A".into()));
        assert!(!capture.should_start(Some("B"),None,OriginalState::Missing,true));
        assert!(!capture.should_start(Some("A"),None,OriginalState::Missing,false));
        assert!(capture.should_start(Some("A"),None,OriginalState::Missing,true));
        assert!(!capture.should_start(Some("A"),None,OriginalState::Missing,true));
        capture.finish(false);
        assert!(!capture.should_start(Some("A"),None,OriginalState::Missing,true));
        capture.retry();
        assert!(capture.should_start(Some("A"),None,OriginalState::Missing,true));
        let mut corrupt = AutoCapture::selected("card".into(),Some("A".into()));
        assert!(!corrupt.should_start(Some("A"),None,OriginalState::Blocked,true));
        corrupt.retry();
        assert_eq!(corrupt.phase,CapturePhase::Blocked);
        let mut existing = AutoCapture::selected("card".into(),Some("A".into()));
        assert!(!existing.should_start(Some("A"),None,OriginalState::Verified,true));
        assert_eq!(existing.phase,CapturePhase::Ready);
    }
    #[test]
    fn offline_unknown_selection_waits_and_known_identity_cannot_cross_devices() {
        let mut capture = AutoCapture::selected("card".into(),None);
        assert!(!capture.should_start(Some("B"),None,OriginalState::Missing,true));
        assert!(!capture.should_start(Some("B"),Some("A"),OriginalState::Missing,true));
        assert!(capture.should_start(Some("A"),Some("A"),OriginalState::Missing,true));
    }
    #[test]
    fn snapshot_render_passes_never_load_disk_and_stale_epochs_are_rejected() {
        let mut cache = SnapshotCache::default();
        let mut disk_loads = 0;
        let load = |calls: &mut usize| { *calls += 1; OriginalState::Verified };
        cache.insert(cache.epoch(),"card".into(),load(&mut disk_loads));
        let ctx = egui::Context::default();
        for _ in 0..100 {
            let _ = ctx.run(Default::default(), |ctx| { egui::CentralPanel::default().show(ctx,|ui| {
                ui.label(format!("{:?}",cache.get("card")));
            }); });
        }
        assert_eq!(disk_loads,1);
        let old = cache.epoch(); cache.invalidate();
        assert!(!cache.insert(old,"card".into(),OriginalState::Verified));
        assert!(cache.get("card").is_none());
    }
}
