//! Cancellable folder metadata reads. This worker never owns or calls the engine.
use super::storage::Result;
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc::{self, Receiver, TryRecvError},
        Arc,
    },
};

pub type Folders = HashMap<String, Vec<String>>;

pub struct ListingJob {
    receiver: Receiver<Result<Folders>>,
    cancelled: Arc<AtomicBool>,
    progress: Arc<AtomicUsize>,
}

impl ListingJob {
    pub fn start(path: PathBuf, roots: Vec<PathBuf>) -> Self {
        Self::spawn(path, roots, |_| {})
    }
    fn spawn(
        path: PathBuf,
        roots: Vec<PathBuf>,
        before_entry: impl Fn(usize) + Send + 'static,
    ) -> Self {
        let (sender, receiver) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let progress = Arc::new(AtomicUsize::new(0));
        let stop = cancelled.clone();
        let count = progress.clone();
        std::thread::spawn(move || {
            let result = (|| {
                let actual = path.canonicalize().map_err(|e| e.to_string())?;
                let roots: Vec<_> = roots.iter().filter_map(|r| r.canonicalize().ok()).collect();
                if !roots.iter().any(|root| actual.starts_with(root)) {
                    return Err(
                        "Choose a folder on Internal, Factory or the mounted USB drive".into(),
                    );
                }
                let names = scan(&path, &stop, &count, &before_entry)?;
                let mut folders = Folders::new();
                // Counts and Patch/Perf classification need only one child level.
                for name in &names {
                    if stop.load(Ordering::Relaxed) {
                        return Err("Listing cancelled".into());
                    }
                    if let Some(name) = name.strip_suffix('/') {
                        let child = path.join(name);
                        if !child
                            .canonicalize()
                            .is_ok_and(|p| roots.iter().any(|root| p.starts_with(root)))
                        {
                            continue;
                        }
                        let entries =
                            scan(&child, &stop, &count, &before_entry).unwrap_or_default();
                        folders.insert(child.to_string_lossy().into_owned(), entries);
                    }
                }
                folders.insert(path.to_string_lossy().into_owned(), names);
                Ok(folders)
            })();
            if !stop.load(Ordering::Relaxed) {
                let _ = sender.send(result);
            }
        });
        Self {
            receiver,
            cancelled,
            progress,
        }
    }
    pub fn progress(&self) -> usize {
        self.progress.load(Ordering::Relaxed)
    }
    pub fn poll(&self) -> Option<Result<Folders>> {
        match self.receiver.try_recv() {
            Ok(result) => Some(result),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(Err("Folder listing stopped".into())),
        }
    }
    #[cfg(test)]
    pub fn controlled(
        path: PathBuf,
        roots: Vec<PathBuf>,
        before_entry: impl Fn(usize) + Send + 'static,
    ) -> Self {
        Self::spawn(path, roots, before_entry)
    }
}
impl Drop for ListingJob {
    fn drop(&mut self) {
        // Dropping a slow read must never wait for its filesystem syscall.
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

fn scan(
    path: &Path,
    stop: &AtomicBool,
    count: &AtomicUsize,
    before_entry: &impl Fn(usize),
) -> Result<Vec<String>> {
    let mut names = vec![];
    for entry in fs::read_dir(path).map_err(|e| format!("{}: {e}", path.display()))? {
        if stop.load(Ordering::Relaxed) {
            return Err("Listing cancelled".into());
        }
        before_entry(count.load(Ordering::Relaxed));
        if stop.load(Ordering::Relaxed) {
            return Err("Listing cancelled".into());
        }
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with('.') {
            names.push(if entry.path().is_dir() {
                format!("{name}/")
            } else {
                name
            });
        }
        count.fetch_add(1, Ordering::Relaxed);
    }
    names.sort_by_cached_key(|s| (!s.ends_with('/'), s.to_lowercase()));
    Ok(names)
}
