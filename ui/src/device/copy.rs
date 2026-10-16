//! File copies for the Files page . A copy touches
//! no engine state, so it runs on the interface thread a slice per frame:
//! the screen keeps updating, counts its progress and can cancel it. Each
//! copied item is assembled under a hidden staging name in the destination
//! and renamed into place when complete, so a cancelled or failed copy never
//! leaves a partial file or folder under a visible name.
use super::storage::Result;
use std::{
    collections::VecDeque,
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

/// Bytes moved per read/write call; a step checks its time after each.
const CHUNK: usize = 256 * 1024;

/// Where a copy stands after a step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Progress {
    /// Top-level items complete.
    pub done: usize,
    pub total: usize,
    pub finished: bool,
}

struct OpenFile {
    from: File,
    to: File,
    target: PathBuf,
}

pub struct CopyJob {
    /// Top-level sources and their final destinations.
    items: Vec<(PathBuf, PathBuf)>,
    done: usize,
    /// The current item's staging path, once started.
    stage: Option<PathBuf>,
    /// Entries of the current item still to copy: (source, staged target).
    queue: VecDeque<(PathBuf, PathBuf)>,
    open: Option<OpenFile>,
    buffer: Vec<u8>,
}

impl CopyJob {
    /// Plan copying `sources` into `folder`. A name that exists there gets a
    /// "Copy N of" prefix, as the instrument's file manager does.
    pub fn new(sources: &[PathBuf], folder: &Path) -> Result<Self> {
        if sources.is_empty() {
            return Err("Nothing to copy".into());
        }
        let target_folder = folder.canonicalize().map_err(|e| e.to_string())?;
        let mut items: Vec<(PathBuf, PathBuf)> = vec![];
        for source in sources {
            let meta = source.symlink_metadata().map_err(|e| e.to_string())?;
            if meta.file_type().is_symlink() {
                return Err("Copy the original file instead of a symbolic link".into());
            }
            if meta.is_dir() {
                let source = source.canonicalize().map_err(|e| e.to_string())?;
                if target_folder.starts_with(&source) {
                    return Err("Cannot copy a folder into itself".into());
                }
            }
            let name = source.file_name().ok_or("Invalid source file")?;
            let mut dest = folder.join(name);
            let mut n = 2;
            while dest.exists() || items.iter().any(|(_, d)| *d == dest) {
                dest = folder.join(format!("Copy {n} of {}", name.to_string_lossy()));
                n += 1;
            }
            items.push((source.clone(), dest));
        }
        Ok(Self {
            items,
            done: 0,
            stage: None,
            queue: VecDeque::new(),
            open: None,
            buffer: vec![0; CHUNK],
        })
    }
    pub fn progress(&self) -> Progress {
        Progress {
            done: self.done,
            total: self.items.len(),
            finished: self.done == self.items.len(),
        }
    }
    /// The final names, in order.
    pub fn destinations(&self) -> impl Iterator<Item = &Path> {
        self.items.iter().map(|(_, d)| d.as_path())
    }
    /// Copy for up to `budget`, then return. Large files continue in the
    /// next step from where this one stopped.
    pub fn step(&mut self, budget: Duration) -> Result<Progress> {
        let started = Instant::now();
        // At least one slice per step, then as many as the budget allows.
        let mut first = true;
        while self.done < self.items.len() {
            if !first && started.elapsed() > budget {
                break;
            }
            first = false;
            if let Some(open) = &mut self.open {
                let n = open
                    .from
                    .read(&mut self.buffer)
                    .map_err(|e| e.to_string())?;
                if n == 0 {
                    open.to.sync_all().map_err(|e| e.to_string())?;
                    self.open = None;
                } else {
                    open.to
                        .write_all(&self.buffer[..n])
                        .map_err(|e| format!("{}: {e}", open.target.display()))?;
                }
                continue;
            }
            if let Some((source, target)) = self.queue.pop_front() {
                self.start_entry(&source, &target)?;
                continue;
            }
            match self.stage.take() {
                // The current item is complete: give it its final name.
                Some(stage) => {
                    let dest = &self.items[self.done].1;
                    fs::rename(&stage, dest).map_err(|e| e.to_string())?;
                    self.done += 1;
                }
                None => {
                    let (source, dest) = &self.items[self.done];
                    let name = dest.file_name().unwrap().to_string_lossy();
                    let stage = dest.with_file_name(format!(".{name}.gr1-ui-copy"));
                    if stage.exists() {
                        return Err(format!("{} is in the way", stage.display()));
                    }
                    self.queue.push_back((source.clone(), stage.clone()));
                    self.stage = Some(stage);
                }
            }
        }
        if self.done == self.items.len() {
            if let Some(parent) = self.items.first().and_then(|(_, d)| d.parent()) {
                File::open(parent)
                    .and_then(|f| f.sync_all())
                    .map_err(|e| e.to_string())?;
            }
        }
        Ok(self.progress())
    }
    fn start_entry(&mut self, source: &Path, target: &Path) -> Result<()> {
        let meta = source.symlink_metadata().map_err(|e| e.to_string())?;
        if meta.file_type().is_symlink() {
            return Err(format!(
                "{}: copy the original file instead of a symbolic link",
                source.display()
            ));
        }
        if meta.is_dir() {
            fs::create_dir(target).map_err(|e| format!("{}: {e}", target.display()))?;
            let mut entries: Vec<_> = fs::read_dir(source)
                .map_err(|e| e.to_string())?
                .collect::<std::io::Result<Vec<_>>>()
                .map_err(|e| e.to_string())?;
            entries.sort_by_key(|e| e.file_name());
            // Depth first, so a folder's files follow it.
            for e in entries.into_iter().rev() {
                self.queue
                    .push_front((e.path(), target.join(e.file_name())));
            }
        } else {
            let from = File::open(source).map_err(|e| e.to_string())?;
            let to = File::create(target).map_err(|e| format!("{}: {e}", target.display()))?;
            self.open = Some(OpenFile {
                from,
                to,
                target: target.into(),
            });
        }
        Ok(())
    }
    /// Stop: completed items stay; the partial item's staging copy, which only
    /// this job wrote, is removed. Returns the number of items completed.
    pub fn cancel(mut self) -> Result<usize> {
        self.open = None;
        if let Some(stage) = self.stage.take() {
            if stage.is_dir() {
                fs::remove_dir_all(&stage).map_err(|e| e.to_string())?;
            } else if stage.exists() {
                fs::remove_file(&stage).map_err(|e| e.to_string())?;
            }
        }
        Ok(self.done)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("gr1-copy-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn tree(root: &Path) {
        fs::create_dir_all(root.join("performance03/000")).unwrap();
        fs::write(
            root.join("performance03/performance.grm"),
            b"[performance]\n",
        )
        .unwrap();
        fs::write(
            root.join("performance03/000/A.wav"),
            vec![7u8; 3 * CHUNK + 5],
        )
        .unwrap();
        fs::create_dir_all(root.join("Drone samples")).unwrap();
        fs::write(root.join("Drone samples/LOW.wav"), b"low").unwrap();
        fs::write(root.join("LOOSE.wav"), b"loose").unwrap();
    }

    #[test]
    fn copies_folders_and_files_in_slices_and_names_clashes() {
        let root = temp("slices");
        let (from, to) = (root.join("internal"), root.join("usb"));
        fs::create_dir_all(&to).unwrap();
        tree(&from);
        fs::write(to.join("LOOSE.wav"), b"other").unwrap();
        let sources: Vec<_> = ["performance03", "Drone samples", "LOOSE.wav"]
            .iter()
            .map(|n| from.join(n))
            .collect();
        let mut job = CopyJob::new(&sources, &to).unwrap();
        assert_eq!(
            job.progress(),
            Progress {
                done: 0,
                total: 3,
                finished: false
            }
        );
        let mut steps = 0;
        loop {
            let p = job.step(Duration::ZERO).unwrap();
            steps += 1;
            if p.finished {
                break;
            }
            // Nothing appears under its final name before it is complete.
            if p.done == 0 {
                assert!(!to.join("performance03").exists());
            }
        }
        assert!(steps > 5, "a zero budget copies one slice per step");
        assert_eq!(
            fs::read(to.join("performance03/000/A.wav")).unwrap(),
            fs::read(from.join("performance03/000/A.wav")).unwrap()
        );
        assert!(to.join("performance03/performance.grm").is_file());
        assert_eq!(fs::read(to.join("Drone samples/LOW.wav")).unwrap(), b"low");
        assert_eq!(fs::read(to.join("LOOSE.wav")).unwrap(), b"other");
        assert_eq!(fs::read(to.join("Copy 2 of LOOSE.wav")).unwrap(), b"loose");
        let hidden: Vec<_> = fs::read_dir(&to)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with('.'))
            .collect();
        assert!(hidden.is_empty(), "no staging copy remains");
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn cancel_keeps_finished_items_and_removes_the_partial_one() {
        let root = temp("cancel");
        let (from, to) = (root.join("internal"), root.join("usb"));
        fs::create_dir_all(&to).unwrap();
        tree(&from);
        let sources = vec![from.join("Drone samples"), from.join("performance03")];
        let mut job = CopyJob::new(&sources, &to).unwrap();
        while job.progress().done == 0 {
            job.step(Duration::ZERO).unwrap();
        }
        // Part of the second item is written.
        for _ in 0..4 {
            job.step(Duration::ZERO).unwrap();
        }
        assert_eq!(job.cancel().unwrap(), 1);
        assert!(to.join("Drone samples/LOW.wav").is_file());
        assert!(!to.join("performance03").exists());
        assert_eq!(fs::read_dir(&to).unwrap().count(), 1);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn refuses_a_folder_into_itself() {
        let root = temp("itself");
        tree(&root);
        let inside = root.join("performance03/000");
        assert!(CopyJob::new(&[root.join("performance03")], &inside).is_err());
        fs::remove_dir_all(&root).unwrap();
    }
}
