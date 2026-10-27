//! Bounded background work and account-scoped state for the design library.
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

const PARALLEL_PREVIEWS: usize = 4;
const AUTOMATIC_ATTEMPTS: u8 = 3;

struct Completion {
    request: u64,
    epoch: u64,
    path: PathBuf,
    result: Result<egui::ColorImage, String>,
}
struct Failure { attempts: u8, retry_at: Instant }

pub struct Previews {
    epoch: u64,
    next_request: u64,
    active: HashMap<u64, (u64, PathBuf)>,
    failures: HashMap<PathBuf, Failure>,
    tx: Sender<Completion>,
    rx: Receiver<Completion>,
}
impl Default for Previews {
    fn default() -> Self {
        let (tx, rx) = channel();
        Self { epoch: 0, next_request: 0, active: HashMap::new(), failures: HashMap::new(), tx, rx }
    }
}
impl Previews {
    /// Old workers retain their slots until completion, even after account changes.
    pub fn reset(&mut self) { self.epoch += 1; self.failures.clear(); }
    pub fn busy(&self) -> bool { !self.active.is_empty() }
    pub fn failed(&self, path: &Path) -> bool { self.failures.contains_key(path) }
    pub fn exhausted(&self, path: &Path) -> bool { self.failures.get(path).is_some_and(|failure| failure.attempts >= AUTOMATIC_ATTEMPTS) }
    pub fn retry(&mut self, path: &Path) { self.failures.remove(path); }
    fn reserve(&mut self, path: &Path, now: Instant) -> Option<(u64, u64)> {
        if self.active.len() >= PARALLEL_PREVIEWS
            || self.active.values().any(|(epoch, active)| *epoch == self.epoch && active == path)
            || self.failures.get(path).is_some_and(|failure| failure.attempts >= AUTOMATIC_ATTEMPTS || now < failure.retry_at) {
            return None;
        }
        self.next_request += 1;
        self.active.insert(self.next_request, (self.epoch, path.to_owned()));
        Some((self.next_request, self.epoch))
    }
    pub fn request(&mut self, path: PathBuf, ctx: &egui::Context,
        load: impl FnOnce() -> anyhow::Result<egui::ColorImage> + Send + 'static) {
        let Some((request, epoch)) = self.reserve(&path, Instant::now()) else { return; };
        let tx = self.tx.clone(); let ctx = ctx.clone();
        std::thread::spawn(move || {
            // A failed decoder must free its slot and remain retryable.
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(load))
                .map_err(|_| "No se pudo generar la miniatura".to_owned())
                .and_then(|result| result.map_err(|_| "No se pudo cargar la imagen".to_owned()));
            let _ = tx.send(Completion { request, epoch, path, result });
            ctx.request_repaint();
        });
    }
    fn finish(&mut self, done: Completion, now: Instant) -> Option<(PathBuf, egui::ColorImage)> {
        self.active.remove(&done.request);
        if done.epoch != self.epoch { return None; }
        match done.result {
            Ok(image) => { self.failures.remove(&done.path); Some((done.path, image)) }
            Err(_) => {
                let attempts = self.failures.get(&done.path).map_or(1, |failure| failure.attempts + 1);
                self.failures.insert(done.path, Failure { attempts, retry_at: now + Duration::from_secs(if attempts == 1 { 2 } else { 8 }) });
                None
            }
        }
    }
    pub fn poll(&mut self) -> Vec<(PathBuf, egui::ColorImage)> {
        let mut ready = Vec::new();
        while let Ok(done) = self.rx.try_recv() {
            if let Some(image) = self.finish(done, Instant::now()) { ready.push(image); }
        }
        ready
    }
}

#[derive(Default)]
pub struct Selection {
    pub path: Option<PathBuf>,
    revision: u64,
    pending: Option<u64>,
}
impl Selection {
    pub fn select(&mut self, path: Option<PathBuf>) {
        if self.path != path { self.revision += 1; self.path = path; }
    }
    pub fn reset(&mut self) { self.revision += 1; self.path = None; self.pending = None; }
    pub fn begin(&mut self) -> Option<u64> {
        if self.path.is_none() || self.pending.is_some() { return None; }
        self.pending = Some(self.revision); self.pending
    }
    pub fn finish(&mut self, request: u64) -> bool {
        if self.pending != Some(request) { return false; }
        self.pending = None;
        self.revision == request && self.path.is_some()
    }
    pub fn busy(&self) -> bool { self.pending.is_some() }
    pub fn cancel(&mut self) { self.pending = None; }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn complete(previews: &mut Previews, ticket: (u64, u64), path: &str, ok: bool, now: Instant) -> bool {
        previews.finish(Completion { request: ticket.0, epoch: ticket.1, path: path.into(),
            result: if ok { Ok(egui::ColorImage::filled([2,2], egui::Color32::RED)) } else { Err("offline".into()) } }, now).is_some()
    }
    #[test]
    fn actual_workers_deliver_fast_image_while_another_image_is_blocked() {
        let mut previews = Previews::default(); let ctx = egui::Context::default();
        let (started_tx, started_rx) = channel(); let (release_tx, release_rx) = channel();
        previews.request("slow".into(), &ctx, move || {
            started_tx.send(()).unwrap(); release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            Ok(egui::ColorImage::filled([2,2],egui::Color32::RED))
        });
        started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        previews.request("fast".into(), &ctx, || Ok(egui::ColorImage::filled([2,2],egui::Color32::BLUE)));
        let fast=previews.rx.recv_timeout(Duration::from_secs(5)).unwrap();assert_eq!(fast.path,Path::new("fast"));
        let (_,image)=previews.finish(fast,Instant::now()).unwrap();assert_eq!(image.pixels[0],egui::Color32::BLUE);
        assert!(previews.busy());release_tx.send(()).unwrap();
        let slow=previews.rx.recv_timeout(Duration::from_secs(5)).unwrap();assert_eq!(slow.path,Path::new("slow"));
        previews.finish(slow,Instant::now()).unwrap();assert!(!previews.busy());
    }
    #[test]
    fn slow_preview_does_not_serialize_gallery_and_account_reset_keeps_bound() {
        let mut previews = Previews::default(); let now = Instant::now();
        let first = previews.reserve(Path::new("slow"), now).unwrap();
        assert!(previews.reserve(Path::new("slow"), now).is_none());
        let fast = previews.reserve(Path::new("fast"), now).unwrap();
        assert!(complete(&mut previews, fast, "fast", true, now));
        let remaining: Vec<_> = (0..3).map(|index| previews.reserve(Path::new(&format!("other-{index}")), now).unwrap()).collect();
        assert!(previews.reserve(Path::new("fifth"), now).is_none());
        previews.reset();
        assert!(previews.reserve(Path::new("new-account"), now).is_none());
        assert!(!complete(&mut previews, first, "slow", true, now));
        assert!(previews.reserve(Path::new("new-account"), now).is_some());
        for (index, ticket) in remaining.into_iter().enumerate() { assert!(!complete(&mut previews, ticket, &format!("other-{index}"), true, now)); }
    }
    #[test]
    fn failed_preview_retries_with_backoff_then_explicit_retry_recovers() {
        let mut previews = Previews::default(); let mut now = Instant::now(); let path = Path::new("retry");
        for _ in 0..3 {
            let ticket = previews.reserve(path, now).unwrap();
            assert!(!complete(&mut previews, ticket, "retry", false, now));
            assert!(previews.failed(path)); assert!(previews.reserve(path, now).is_none());
            now += Duration::from_secs(10);
        }
        assert!(previews.reserve(path, now).is_none());
        previews.retry(path);
        let ticket = previews.reserve(path, now).unwrap();
        assert!(complete(&mut previews, ticket, "retry", true, now)); assert!(!previews.failed(path));
    }
    #[test]
    fn use_requires_selection_rejects_duplicate_and_stale_results() {
        let mut selection = Selection::default(); assert!(selection.begin().is_none());
        selection.select(Some("first".into())); let request = selection.begin().unwrap();
        assert!(selection.begin().is_none()); assert!(selection.busy());
        selection.select(Some("second".into())); assert!(!selection.finish(request));
        let current = selection.begin().unwrap(); assert!(selection.finish(current));
        assert_eq!(selection.path, Some("second".into()));
        selection.begin().unwrap(); selection.cancel();
        assert!(!selection.busy()); assert_eq!(selection.path, Some("second".into()));
        let obsolete = selection.begin().unwrap(); selection.reset(); selection.select(Some("first".into()));
        assert!(!selection.finish(obsolete));
    }
}
