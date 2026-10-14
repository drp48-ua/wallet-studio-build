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
pub enum CapturePhase { Waiting, Running, Ready, Failed, Blocked, ForeignOriginal }
/// Called only for a new human selection. An actually connected selection wins
/// over a hint about a saved baseline on another device.
pub fn capture_device_for_selection(connected_device: Option<&str>, known_offline_device: Option<&str>) -> Option<String> {
    connected_device.or(known_offline_device).map(str::to_string)
}
pub struct AutoCapture { pub hash: String, pub device: Option<String>, pub phase: CapturePhase }
impl AutoCapture {
    pub fn selected(hash: String, device: Option<String>) -> Self { Self { hash, device, phase: CapturePhase::Waiting } }
    pub fn should_start(&mut self, selected_device: Option<&str>, known_device: Option<&str>,
        original: OriginalState, safe: bool) -> bool {
        if self.phase != CapturePhase::Waiting { return false; }
        // Never retarget an existing human selection. Offline discovery can
        // resolve an unbound request only while no device is selected.
        if self.device.is_none() && selected_device.is_none() {
            self.device = known_device.map(str::to_string);
        }
        if selected_device.is_some() && self.device.as_deref() != selected_device { return false; }
        if selected_device.is_none() && known_device.is_some() && self.device.as_deref() != known_device { return false; }
        if original == OriginalState::Blocked { self.phase = CapturePhase::Blocked; return false; }
        if original == OriginalState::Verified { self.phase = CapturePhase::Ready; return false; }
        // A baseline for the same card on another phone does not prove that
        // this phone's current Combined assets are still the original artwork.
        if selected_device.is_some() && known_device.is_some() && selected_device != known_device {
            self.phase = CapturePhase::ForeignOriginal; return false;
        }
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
        assert_eq!(capture.device,None);
        assert!(!capture.should_start(None,Some("A"),OriginalState::Missing,false));
        assert!(capture.should_start(Some("A"),Some("A"),OriginalState::Missing,true));
    }
    #[test]
    fn connected_human_selection_wins_over_another_devices_baseline_hint() {
        let device = capture_device_for_selection(Some("B"),Some("A"));
        assert_eq!(device.as_deref(),Some("B"));
        let mut capture = AutoCapture::selected("card".into(),device);
        assert!(!capture.should_start(Some("B"),Some("A"),OriginalState::Missing,true));
        assert_eq!(capture.phase,CapturePhase::ForeignOriginal);
        assert_eq!(capture.device.as_deref(),Some("B"));
        capture.retry();
        assert_eq!(capture.phase,CapturePhase::ForeignOriginal);
        let mut verified = AutoCapture::selected("card".into(),Some("B".into()));
        assert!(!verified.should_start(Some("B"),Some("A"),OriginalState::Verified,true));
        assert_eq!(verified.phase,CapturePhase::Ready);
        let mut blocked = AutoCapture::selected("card".into(),Some("B".into()));
        assert!(!blocked.should_start(Some("B"),Some("A"),OriginalState::Blocked,true));
        assert_eq!(blocked.phase,CapturePhase::Blocked);
    }
    #[test]
    fn offline_selection_is_not_retargeted_by_connection_or_another_hint() {
        let mut capture = AutoCapture::selected("card".into(),capture_device_for_selection(None,Some("A")));
        assert!(!capture.should_start(Some("B"),Some("B"),OriginalState::Verified,true));
        assert_eq!(capture.phase,CapturePhase::Waiting);
        assert_eq!(capture.device.as_deref(),Some("A"));
        assert!(capture.should_start(Some("A"),Some("A"),OriginalState::Missing,true));
    }
    #[test]
    fn device_switch_rejects_old_snapshot_and_requires_new_human_binding() {
        let mut cache = SnapshotCache::default();
        let epoch_a = cache.epoch();
        cache.insert(epoch_a,"card".into(),("A",OriginalState::Verified));
        let mut request_a = AutoCapture::selected("card".into(),Some("A".into()));
        cache.invalidate();
        assert!(!cache.insert(epoch_a,"card".into(),("A",OriginalState::Verified)));
        assert!(cache.insert(cache.epoch(),"card".into(),("B",OriginalState::Missing)));
        let (device,state) = *cache.get("card").unwrap();
        assert!(!request_a.should_start(Some(device),Some("A"),state,true));
        assert_eq!(request_a.device.as_deref(),Some("A"));
        let mut request_b = AutoCapture::selected("card".into(),capture_device_for_selection(Some(device),Some("A")));
        assert!(!request_b.should_start(Some(device),Some("A"),state,true));
        assert_eq!(request_b.phase,CapturePhase::ForeignOriginal);
        let mut fresh_request_b = AutoCapture::selected("card".into(),Some("B".into()));
        assert!(fresh_request_b.should_start(Some(device),None,state,true));
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
