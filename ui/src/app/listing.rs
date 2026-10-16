//! UI lifecycle of the shared metadata reader. Old rows stay usable after Cancel.
use super::*;
use crate::device::listing::{Folders, ListingJob};

pub(super) enum Target {
    Browser {
        place: overlay::Place,
        focus: Option<String>,
    },
    Files {
        path: String,
        focus: Option<String>,
        at_locations: bool,
    },
}
pub(super) struct Listing {
    pub job: ListingJob,
    pub target: Target,
}

impl App {
    pub(super) fn complete_unmount(
        &mut self,
        result: Result<(), String>,
        path: String,
        job: Option<ListingJob>,
    ) -> Result<(), String> {
        result?;
        self.cancel_listing();
        let at_locations = self.perf.files.at_locations;
        let names = self
            .files_ui
            .folders
            .get(&path)
            .cloned()
            .unwrap_or_default();
        if let Some(d) = &mut self.device {
            d.accept_listing(path.clone().into(), names.clone());
        }
        self.usb_mounted = false;
        self.directory = path.clone();
        self.files = names;
        self.perf.files.selection.clear();
        self.perf_files_focus_opt(None);
        self.perf_files_relist();
        if let Some(job) = job {
            self.begin_listing(
                job,
                Target::Files {
                    path,
                    focus: None,
                    at_locations,
                },
            );
        }
        Ok(())
    }
    pub(super) fn begin_listing(&mut self, job: ListingJob, target: Target) {
        self.files_ui.preview_due = None;
        self.files_ui.listing = Some(Listing { job, target });
        self.notify("Listing folder: 0 entries");
        self.navigation_dirty = true;
    }
    pub(super) fn cancel_listing(&mut self) -> bool {
        if self.files_ui.listing.take().is_none() {
            return false;
        }
        self.notify("Listing cancelled");
        self.navigation_dirty = true;
        true
    }
    pub(super) fn listing_frame(&mut self) -> bool {
        let Some(listing) = self.files_ui.listing.as_ref() else {
            return false;
        };
        let result = listing.job.poll();
        let progress = listing.job.progress();
        let Some(result) = result else {
            self.notify(&format!("Listing folder: {progress} entries"));
            return true;
        };
        let listing = self.files_ui.listing.take().unwrap();
        match result {
            Ok(folders) => self.finish_listing(listing.target, folders),
            Err(e) => self.error(&e),
        }
        self.navigation_dirty = true;
        true
    }
    fn finish_listing(&mut self, target: Target, folders: Folders) {
        self.files_ui.folders.extend(folders);
        self.files_ui.info.clear();
        match target {
            Target::Browser { place, focus } => self.show_browser_place(place, focus),
            Target::Files {
                path,
                focus,
                at_locations,
            } => {
                let same_place = self.directory == path && !self.perf.files.at_locations;
                let names = self
                    .files_ui
                    .folders
                    .get(&path)
                    .cloned()
                    .unwrap_or_default();
                if let Some(d) = &mut self.device {
                    d.accept_listing(path.clone().into(), names.clone());
                }
                self.directory = path;
                self.files = names;
                self.perf.files.at_locations = at_locations;
                if !same_place {
                    self.perf.files.selection.clear();
                }
                self.perf_files_focus_opt(focus);
                self.perf_files_relist();
            }
        }
        self.notify("Folder listed");
    }
}

#[cfg(test)]
mod tests {
    use super::super::panel_tests::Panel;
    use super::*;
    use std::{fs, sync::mpsc, time::SystemTime};

    struct Scratch(std::path::PathBuf);
    static NEXT_SCRATCH: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    impl Scratch {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "gr1-listing-{}-{}-{}",
                std::process::id(),
                NEXT_SCRATCH.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
                SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn slow_real_folder_listing_reports_progress_cancels_and_can_complete() {
        let scratch = Scratch::new();
        for i in 0..250 {
            fs::write(scratch.0.join(format!("clip-{i:03}.wav")), b"fixture").unwrap();
        }
        fs::create_dir(scratch.0.join("patch")).unwrap();
        fs::write(scratch.0.join("patch/sound.grp"), b"fixture").unwrap();
        let path = scratch.0.to_string_lossy().into_owned();
        for browser in [true, false] {
            let mut p = Panel::new();
            if browser {
                p.events(&[(1, 0), (5, 2)]);
            } else {
                p.events(&[(12, 0), (12, 0), (12, 0), (12, 0), (5, 2)]);
            }
            let prior = p.app.directory.clone();
            let prior_files = p.app.files.clone();
            let prior_cursor = p.app.perf.files.list;
            let prior_browser = match p.app.files_ui.layers.last() {
                Some(overlay::Layer::Browser(b)) => Some((b.place.clone(), b.list)),
                _ => None,
            };
            let (entered_tx, entered_rx) = mpsc::channel();
            let (release_tx, release_rx) = mpsc::channel();
            let release_rx = std::sync::Mutex::new(release_rx);
            let job =
                ListingJob::controlled(scratch.0.clone(), vec![scratch.0.clone()], move |n| {
                    if n == 12 {
                        entered_tx.send(()).unwrap();
                        release_rx.lock().unwrap().recv().unwrap();
                    }
                });
            let target = || {
                if browser {
                    Target::Browser {
                        place: overlay::Place::Folder(path.clone()),
                        focus: Some("clip-120.wav".into()),
                    }
                } else {
                    Target::Files {
                        path: path.clone(),
                        focus: Some("clip-120.wav".into()),
                        at_locations: false,
                    }
                }
            };
            p.app.begin_listing(job, target());
            entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
            p.app.listing_frame();
            p.app.render(&mut p.painter);
            assert_eq!(p.app.toast, "Listing folder: 12 entries");
            assert_eq!(p.app.directory, prior);
            let started = Instant::now();
            p.events(&[(14, 0)]);
            let elapsed = started.elapsed();
            assert!(
                elapsed < Duration::from_millis(250),
                "Escape waited {elapsed:?} for a blocked metadata read"
            );
            assert!(p.app.files_ui.listing.is_none());
            assert_eq!(p.app.directory, prior);
            assert_eq!(p.app.files, prior_files);
            assert_eq!(p.app.perf.files.list, prior_cursor);
            if let Some((place, cursor)) = prior_browser {
                match p.app.files_ui.layers.last() {
                    Some(overlay::Layer::Browser(b)) => {
                        assert_eq!((b.place.clone(), b.list), (place, cursor))
                    }
                    _ => panic!("Cancel lost the previous browser"),
                }
            }
            release_tx.send(()).unwrap();
            p.app.begin_listing(
                ListingJob::start(scratch.0.clone(), vec![scratch.0.clone()]),
                target(),
            );
            let deadline = Instant::now() + Duration::from_secs(3);
            while p.app.files_ui.listing.is_some() && Instant::now() < deadline {
                p.app.listing_frame();
                std::thread::yield_now();
            }
            assert!(p.app.files_ui.listing.is_none());
            p.app.render(&mut p.painter);
            if browser {
                match p.app.files_ui.layers.last() {
                    Some(overlay::Layer::Browser(b)) => {
                        assert_eq!(b.place, overlay::Place::Folder(path.clone()));
                        assert_eq!(b.rows.len(), 251);
                        assert_eq!(b.rows[b.list.focus].key, "clip-120.wav");
                        assert_eq!(
                            b.rows.iter().find(|r| r.key == "patch/").unwrap().right,
                            "1"
                        );
                    }
                    _ => panic!(),
                }
            } else {
                assert_eq!(p.app.directory, path);
                assert_eq!(p.app.files.len(), 251);
                assert_eq!(
                    p.app.perf.files.rows[p.app.perf.files.list.focus].key,
                    "clip-120.wav"
                );
            }
            println!("{} listing: real 251-entry folder {}, progress=12, Escape={elapsed:?}, prior folder/focus retained; resumed listing completed", if browser { "U2" } else { "U4" }, path);
        }
    }

    #[test]
    fn unmount_result_returns_internal_and_refresh_remains_cancellable() {
        let scratch = Scratch::new();
        for i in 0..20 {
            fs::write(scratch.0.join(format!("file-{i}.txt")), b"fixture").unwrap();
        }
        let path = scratch.0.to_string_lossy().into_owned();
        for at_locations in [false, true] {
            let mut p = Panel::new();
            p.events(&[(12, 0), (12, 0), (12, 0), (12, 0)]);
            p.app.directory = "/USB/session".into();
            p.app.files = vec!["USB-only.wav".into()];
            p.app.perf.files.at_locations = at_locations;
            p.app.perf_files_focus_opt(None);
            p.app.perf_files_relist();
            p.app.perf.files.selection.insert("USB-only.wav".into());
            let cursor = p.app.perf.files.list;
            // The command result is injected; no umount or native Device runs.
            let error = p
                .app
                .complete_unmount(Err("umount failed".into()), path.clone(), None)
                .unwrap_err();
            assert_eq!(error, "umount failed");
            assert_eq!(p.app.directory, "/USB/session");
            assert_eq!(p.app.perf.files.list, cursor);
            assert!(p.app.usb_mounted);
            assert!(p.app.perf.files.selection.contains("USB-only.wav"));
            let (entered_tx, entered_rx) = mpsc::channel();
            let (release_tx, release_rx) = mpsc::channel();
            let release_rx = std::sync::Mutex::new(release_rx);
            let job =
                ListingJob::controlled(scratch.0.clone(), vec![scratch.0.clone()], move |n| {
                    if n == 0 {
                        entered_tx.send(()).unwrap();
                        release_rx.lock().unwrap().recv().unwrap();
                    }
                });
            p.app
                .complete_unmount(Ok(()), path.clone(), Some(job))
                .unwrap();
            entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
            assert_eq!(p.app.directory, path);
            assert!(!p.app.files.contains(&"USB-only.wav".into()));
            assert!(!p.app.usb_mounted);
            assert!(p.app.perf.files.selection.is_empty());
            assert_eq!(p.app.perf.files.at_locations, at_locations);
            let started = Instant::now();
            p.events(&[(14, 0)]);
            assert!(started.elapsed() < Duration::from_millis(250));
            assert!(p.app.files_ui.listing.is_none());
            assert_eq!(
                p.app.directory, path,
                "Cancel cannot return to an unmounted USB folder"
            );
            release_tx.send(()).unwrap();
            p.app.begin_listing(
                ListingJob::start(scratch.0.clone(), vec![scratch.0.clone()]),
                Target::Files {
                    path: path.clone(),
                    focus: None,
                    at_locations,
                },
            );
            let deadline = Instant::now() + Duration::from_secs(3);
            while p.app.files_ui.listing.is_some() && Instant::now() < deadline {
                p.app.listing_frame();
                std::thread::yield_now();
            }
            assert!(p.app.files_ui.listing.is_none());
            assert_eq!(p.app.files.len(), 20);
            assert_eq!(p.app.perf.files.at_locations, at_locations);
            assert!(!p
                .app
                .perf
                .files
                .rows
                .iter()
                .any(|r| r.key == "location1" || r.key == "USB-only.wav"));
            println!("Unmount result: failure retained USB; success entered Internal, Escape cancelled metadata read, resumed 20-file refresh preserved locations={at_locations}");
        }
    }

    #[test]
    fn consecutive_nested_recorded_takes_survive_cancelled_files_refresh() {
        let scratch = Scratch::new();
        fs::write(scratch.0.join("gate.txt"), b"fixture").unwrap();
        let mut p = Panel::new();
        let nested = "/Internal/Deep/Takes";
        p.app.files.push("Deep/".into());
        p.app
            .folders
            .insert("/Internal/Deep".into(), vec!["Takes/".into()]);
        p.app.folders.insert(nested.into(), vec![]);
        p.app.files_ui.folders.insert(nested.into(), vec![]);
        // Record/Stop, Save sample, destination picker, Deep > Takes, Save.
        p.events(&[(15, 0), (7, 0), (7, 0), (5, 2), (7, 0), (5, 255), (7, 0)]);
        p.events(&[(7, 0), (7, 0), (2, 1), (2, 1)]);
        assert!(!p.app.overlay_active());
        assert_eq!(
            p.app.directory, "/Internal",
            "Files is elsewhere than the save folder"
        );
        assert_eq!(p.app.files_ui.folders[nested], ["REEDS_04-2.wav"]);
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let release_rx = std::sync::Mutex::new(release_rx);
        let job = ListingJob::controlled(scratch.0.clone(), vec![scratch.0.clone()], move |n| {
            if n == 0 {
                entered_tx.send(()).unwrap();
                release_rx.lock().unwrap().recv().unwrap();
            }
        });
        p.app.begin_listing(
            job,
            Target::Files {
                path: "/Internal".into(),
                focus: None,
                at_locations: false,
            },
        );
        entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        p.events(&[(14, 0)]);
        release_tx.send(()).unwrap();
        // Record a second take after cancelling the unrelated Files refresh.
        p.events(&[(5, 254), (7, 0), (7, 0), (5, 2), (7, 0)]);
        match p.app.files_ui.layers.last() {
            Some(overlay::Layer::Save(s)) => {
                assert_eq!((s.folder.as_str(), s.name.as_str()), (nested, "REEDS_04-3"))
            }
            _ => panic!("second recording opens Save"),
        }
        p.events(&[(2, 1)]);
        assert!(
            !p.app.overlay_active(),
            "the new take must not ask to replace the first"
        );
        assert_eq!(
            p.app.files_ui.folders[nested],
            ["REEDS_04-2.wav", "REEDS_04-3.wav"]
        );
        assert_eq!(p.app.directory, "/Internal");
        assert_eq!(p.app.toast, "Saved REEDS_04-3.wav");
        // The device's chooser also observes existing files even if a cached
        // listing misses them; it probes candidate WAV targets individually.
        fs::write(scratch.0.join("REEDS_04-2.wav"), b"previous take").unwrap();
        fs::write(scratch.0.join("REEDS_04-3.wav"), b"previous take").unwrap();
        assert_eq!(
            App::take_name_with("REEDS_04-2", |candidate| {
                crate::device::storage::sample_file_name(candidate)
                    .is_ok_and(|file| scratch.0.join(file).exists())
            }),
            "REEDS_04-4"
        );
    }
}
