//! The Files page of the Perf root . The Files directory and listing stay the device's (or the desktop
//! demo's); this page keeps the interface state: the focused row by name,
//! each folder's remembered focus, the selection and the running paste.
//! Selection follows stock (manual p.74): holding Shift selects the focused
//! row and every row turned onto; a press also toggles one row (decided
//! 2026-10-12). Copy registers the selection; Copy in another folder pastes
//! after a confirmation.
use super::components::ListCursor;
use super::overlay::{is_audio, Place};
use super::perf_root::*;
use super::*;
use crate::device::copy::CopyJob;
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq)]
pub(super) struct FileRow {
    /// The name, with '/' for a folder, or "locationN".
    pub key: String,
    pub label: String,
    pub folder: bool,
    /// A folder's item count.
    pub count: String,
    pub location: Option<usize>,
}

pub(super) enum PasteJob {
    Files {
        job: CopyJob,
        into: String,
    },
    /// The desktop demo copies one listed item per frame.
    Demo {
        items: Vec<String>,
        done: usize,
        into: String,
    },
}

#[derive(Default)]
pub(super) struct FilesPage {
    /// Above the location roots: Internal, the USB stick, Factory.
    pub at_locations: bool,
    pub rows: Vec<FileRow>,
    /// What the rows were built from: (at locations, directory, names).
    listed: Option<(bool, String, Vec<String>)>,
    pub list: ListCursor,
    pub selection: BTreeSet<String>,
    /// Focused row key and first visible row per folder.
    memory: HashMap<String, (String, usize)>,
    pub job: Option<PasteJob>,
    /// The row to focus at the next listing (a renamed or new entry).
    want: Option<String>,
}

fn join(folder: &str, name: &str) -> String {
    format!(
        "{}/{}",
        folder.trim_end_matches('/'),
        name.trim_end_matches('/')
    )
}

impl App {
    /// Rebuild the rows when the listing changed, keeping focus by name.
    pub(super) fn perf_files_relist(&mut self) {
        let at = self.perf.files.at_locations;
        let names: Vec<String> = if at {
            (0..3)
                .filter(|i| self.location_root(*i).is_some())
                .map(|i| format!("location{i}"))
                .collect()
        } else {
            self.files.clone()
        };
        let listing = (at, self.directory.clone(), names);
        let want = self.perf.files.want.take();
        if want.is_none() && self.perf.files.listed.as_ref() == Some(&listing) {
            return;
        }
        let old = self
            .perf
            .files
            .rows
            .get(self.perf.files.list.focus)
            .map(|r| r.key.clone());
        let same_place = self
            .perf
            .files
            .listed
            .as_ref()
            .is_some_and(|l| l.0 == listing.0 && l.1 == listing.1);
        let rows: Vec<FileRow> = if at {
            listing
                .2
                .iter()
                .map(|key| {
                    let i = key[8..].parse::<usize>().unwrap_or(0);
                    FileRow {
                        key: key.clone(),
                        label: self.location_label(i),
                        folder: true,
                        count: String::new(),
                        location: Some(i),
                    }
                })
                .collect()
        } else {
            let mut names = listing.2.clone();
            names.sort_by_key(|s| (!s.ends_with('/'), s.to_lowercase()));
            names
                .iter()
                .map(|name| {
                    let folder = name.ends_with('/');
                    FileRow {
                        key: name.clone(),
                        label: name.trim_end_matches('/').to_string(),
                        folder,
                        count: if folder {
                            if self.device.is_some() {
                                self.files_ui
                                    .folders
                                    .get(&join(&self.directory, name))
                                    .map(|n| n.len().to_string())
                                    .unwrap_or_default()
                            } else {
                                self.list_folder(&join(&self.directory, name))
                                    .map(|n| n.len().to_string())
                                    .unwrap_or_default()
                            }
                        } else {
                            String::new()
                        },
                        location: None,
                    }
                })
                .collect()
        };
        let files = &mut self.perf.files;
        let memory = if at {
            files.memory.get("locations").cloned()
        } else {
            files.memory.get(&listing.1).cloned()
        };
        let found = |key: &str| {
            rows.iter()
                .position(|r| r.key == key || r.key.trim_end_matches('/') == key)
        };
        let focus = want
            .as_deref()
            .and_then(found)
            .or_else(|| old.filter(|_| same_place).as_deref().and_then(found))
            .or_else(|| memory.as_ref().and_then(|m| found(&m.0)));
        files.list.focus = match focus {
            Some(i) => i,
            // A vanished row: its nearest neighbor takes focus (3.5.3).
            None if same_place => files.list.focus,
            None => 0,
        };
        if !same_place {
            files.list.top = memory.map_or(0, |m| m.1);
        }
        files.selection.retain(|k| rows.iter().any(|r| r.key == *k));
        files.rows = rows;
        files.list.reveal(files.rows.len());
        files.listed = Some(listing);
    }
    fn perf_files_remember(&mut self) {
        let files = &mut self.perf.files;
        if let Some(row) = files.rows.get(files.list.focus) {
            let key = if files.at_locations {
                "locations".to_string()
            } else {
                self.directory.clone()
            };
            files.memory.insert(key, (row.key.clone(), files.list.top));
        }
    }
    /// Enter `path`; entering or leaving a folder clears the selection, as
    /// stock does. `focus` names the row to focus there.
    fn perf_files_cd(&mut self, path: &str, focus: Option<String>) {
        self.perf_files_remember();
        if let Some(d) = &self.device {
            let job = d.start_listing(path.into());
            self.begin_listing(
                job,
                super::listing::Target::Files {
                    path: path.into(),
                    focus,
                    at_locations: false,
                },
            );
            return;
        }
        let result = match &mut self.device {
            Some(d) => d.change_directory(path.into()),
            None => {
                self.change_directory(path.into());
                Ok(())
            }
        };
        match result {
            Ok(()) => {
                self.sync_device_view();
                self.perf.files.at_locations = false;
                self.perf.files.selection.clear();
                self.perf.files.want = focus;
                self.perf.files.listed = None;
                self.perf_files_relist();
            }
            Err(e) => self.error(&e),
        }
    }
    fn perf_files_focused(&self) -> Option<FileRow> {
        self.perf
            .files
            .rows
            .get(self.perf.files.list.focus)
            .cloned()
    }
    fn perf_files_path(&self, row: &FileRow) -> String {
        join(&self.directory, &row.key)
    }
    fn perf_files_writable(&self) -> bool {
        !self
            .location_of(&self.directory)
            .is_some_and(|(i, _)| i == 2)
    }

    pub(super) fn perf_files_turn(&mut self, delta: i32) {
        self.perf_files_relist();
        let held = self.shift_held();
        let files = &mut self.perf.files;
        let len = files.rows.len();
        if held && !files.at_locations {
            // With Shift held the engine reports one detent as 10; in Files
            // one such report moves one row and selects it (stock).
            files.list.turn(delta.signum(), len);
            if let Some(row) = files.rows.get(files.list.focus) {
                files.selection.insert(row.key.clone());
            }
        } else {
            files.list.turn(delta, len);
        }
    }
    /// Holding Shift selects the focused row.
    pub(super) fn perf_shift_select(&mut self) {
        if self.tab != 4
            || self.perf.page != FILES
            || self.perf.strip
            || !self.perf.layers.is_empty()
            || self.perf.grab.is_some()
            || !self.shift_held()
        {
            return;
        }
        self.perf_files_relist();
        let files = &mut self.perf.files;
        if files.at_locations {
            return;
        }
        if let Some(row) = files.rows.get(files.list.focus) {
            files.selection.insert(row.key.clone());
            self.navigation_dirty = true;
        }
    }
    /// Press: a location or folder opens; a file toggles its selection.
    pub(super) fn perf_files_press(&mut self) {
        self.perf_files_relist();
        let Some(row) = self.perf_files_focused() else {
            return;
        };
        if let Some(i) = row.location {
            if let Some(root) = self.location_root(i) {
                let memory = self.perf.files.memory.get(&root).map(|m| m.0.clone());
                self.perf_files_cd(&root, memory);
            }
        } else if row.folder {
            let path = self.perf_files_path(&row);
            let memory = self.perf.files.memory.get(&path).map(|m| m.0.clone());
            self.perf_files_cd(&path, memory);
        } else {
            let selection = &mut self.perf.files.selection;
            if !selection.remove(&row.key) {
                selection.insert(row.key);
            }
        }
    }
    /// Escape climbs one folder, then to the locations; true when it did.
    pub(super) fn perf_files_escape(&mut self) -> bool {
        if self.perf.files.at_locations {
            return false;
        }
        let dir = self.directory.clone();
        match self.location_of(&dir) {
            Some((i, root)) if root == dir => {
                self.perf_files_remember();
                let files = &mut self.perf.files;
                files.at_locations = true;
                files.selection.clear();
                files.want = Some(format!("location{i}"));
                files.listed = None;
                self.perf_files_relist();
            }
            _ => {
                let Some((parent, name)) = dir.rsplit_once('/') else {
                    return false;
                };
                let (parent, name) = (parent.to_string(), format!("{name}/"));
                self.perf_files_cd(&parent, Some(name));
            }
        }
        true
    }
    /// Shift+press: the row's edit sheet .
    pub(super) fn perf_files_sheet(&mut self) {
        self.perf_files_relist();
        let Some(row) = self.perf_files_focused() else {
            if !self.perf.files.at_locations && self.perf_files_writable() {
                self.perf_sheet(
                    self.place_title(&Place::Folder(self.directory.clone())),
                    vec![Item {
                        label: "New folder".into(),
                        keycap: None,
                        destructive: false,
                        act: Do::NewFolder,
                    }],
                );
            }
            return;
        };
        let item = |label: String, destructive, act| Item {
            label,
            keycap: None,
            destructive,
            act,
        };
        let mut items = vec![];
        if let Some(i) = row.location {
            items.push(item("Open".into(), false, Do::Open));
            if i == 1 {
                items.push(item("Unmount".into(), false, Do::Unmount));
            }
        } else {
            let files = &self.perf.files;
            let selected = files.selection.contains(&row.key);
            items.push(item(
                if selected { "Unselect" } else { "Select" }.into(),
                false,
                Do::Select,
            ));
            if files.selection.len() < files.rows.len() {
                items.push(item("Select all".into(), false, Do::SelectAll));
            }
            if !files.selection.is_empty() {
                items.push(item("Unselect all".into(), false, Do::UnselectAll));
            }
            if self.perf_files_writable() {
                items.push(item("Rename".into(), false, Do::RenameFile));
                items.push(item("New folder".into(), false, Do::NewFolder));
                let count = self.perf_files_targets().len();
                items.push(item(
                    if count > 1 {
                        format!("Delete {count} selected")
                    } else {
                        "Delete".into()
                    },
                    true,
                    Do::Delete,
                ));
            }
        }
        self.perf_sheet(row.label.clone(), items);
    }
    /// The rows an action acts on: the selection, else the focused row.
    fn perf_files_targets(&self) -> Vec<String> {
        let files = &self.perf.files;
        if files.selection.is_empty() {
            files
                .rows
                .get(files.list.focus)
                .map(|r| vec![r.key.clone()])
                .unwrap_or_default()
        } else {
            files
                .rows
                .iter()
                .filter(|r| files.selection.contains(&r.key))
                .map(|r| r.key.clone())
                .collect()
        }
    }
    pub(super) fn perf_files_do(&mut self, act: Do) {
        let Some(row) = self.perf_files_focused() else {
            if act == Do::NewFolder {
                self.open_name_editor(overlay::NamePurpose::NewFolder);
            }
            return;
        };
        match act {
            Do::Open => self.perf_files_press(),
            Do::Select => {
                let selection = &mut self.perf.files.selection;
                if !selection.remove(&row.key) {
                    selection.insert(row.key);
                }
            }
            Do::SelectAll => {
                let keys: Vec<String> =
                    self.perf.files.rows.iter().map(|r| r.key.clone()).collect();
                self.perf.files.selection.extend(keys);
            }
            Do::UnselectAll => self.perf.files.selection.clear(),
            Do::RenameFile => {
                let path = self.perf_files_path(&row);
                self.open_name_editor(overlay::NamePurpose::Rename(path));
            }
            Do::NewFolder => self.open_name_editor(overlay::NamePurpose::NewFolder),
            Do::Delete => {
                let targets = self.perf_files_targets();
                let lines = match targets.as_slice() {
                    [one] => vec![format!("Delete {}", one.trim_end_matches('/'))],
                    many => vec![
                        format!("Delete {} items", many.len()),
                        format!(
                            "in {}",
                            self.place_title(&Place::Folder(self.directory.clone()))
                        ),
                    ],
                };
                let paths = targets.iter().map(|k| join(&self.directory, k)).collect();
                self.perf_ask(
                    lines,
                    "They move to this folder's trash.",
                    "Delete",
                    "",
                    Verb::Delete(paths),
                );
            }
            Do::Unmount => self.perf_ask(
                vec![format!("Unmount {}", row.label)],
                "Unfinished file writes end first.",
                "Unmount",
                "",
                Verb::Op("unmount"),
            ),
            _ => {}
        }
    }
    pub(super) fn perf_files_delete(&mut self, paths: &[String]) {
        let mut failed = None;
        for path in paths {
            let result = match &mut self.device {
                Some(d) => d.trash_path(Path::new(path)),
                None => self.demo_edit(path, None),
            };
            if let Err(e) = result {
                failed = Some(e);
                break;
            }
        }
        self.refresh_files_listing();
        self.sync_device_view();
        self.perf.files.selection.clear();
        self.perf_files_relist();
        match failed {
            Some(e) => self.error(&format!("Delete failed: {e}")),
            None => self.notify(&match paths {
                [one] => format!(
                    "Moved {} to the trash",
                    one.rsplit('/').next().unwrap_or_default()
                ),
                many => format!("Moved {} items to the trash", many.len()),
            }),
        }
    }
    /// A renamed or new entry: focus it at the next listing.
    pub(super) fn perf_files_focus(&mut self, name: &str) {
        self.perf_files_focus_opt(Some(name.into()));
    }
    pub(super) fn perf_files_focus_opt(&mut self, focus: Option<String>) {
        if let Some(super::listing::Listing {
            target: super::listing::Target::Files { focus: pending, .. },
            ..
        }) = self.files_ui.listing.as_mut()
        {
            *pending = focus.clone();
        }
        self.perf.files.want = focus;
        self.perf.files.listed = None;
    }
    pub(super) fn perf_files_unmounted(&mut self) {
        self.perf.files.listed = None;
        if !self.perf.files.at_locations {
            self.perf.files.selection.clear();
        }
        self.perf_files_relist();
    }

    // ----- copy and paste -----

    /// Copy registers the selection (or the focused row); with nothing
    /// selected and items registered, it asks to paste them here.
    pub(super) fn perf_files_copy(&mut self) {
        self.perf_files_relist();
        if self.perf.files.job.is_some() || self.perf.files.at_locations {
            return;
        }
        let selected = !self.perf.files.selection.is_empty();
        if !selected && !self.clipboard.is_empty() {
            let count = self.clipboard.len();
            let first = self.clipboard[0]
                .trim_end_matches('/')
                .rsplit('/')
                .next()
                .unwrap_or_default()
                .to_string();
            self.perf_ask(
                vec![
                    if count == 1 {
                        format!("Copy {first}")
                    } else {
                        format!("Copy {count} items")
                    },
                    format!(
                        "into {}",
                        self.place_title(&Place::Folder(self.directory.clone()))
                    ),
                ],
                "Nothing is replaced; a name that exists gets a Copy prefix.",
                "Copy",
                "Copy",
                Verb::Paste,
            );
            return;
        }
        let keys = self.perf_files_targets();
        if keys.is_empty() {
            return;
        }
        match &mut self.device {
            Some(d) => {
                d.clipboard = keys
                    .iter()
                    .map(|k| d.directory.join(k.trim_end_matches('/')))
                    .collect();
            }
            None => {
                self.clipboard = keys
                    .iter()
                    .map(|k| {
                        let path = join(&self.directory, k);
                        if k.ends_with('/') {
                            format!("{path}/")
                        } else {
                            path
                        }
                    })
                    .collect();
            }
        }
        self.sync_device_view();
        self.notify(&match keys.as_slice() {
            [one] => format!("Copied {}", one.trim_end_matches('/')),
            many => format!("Copied {} items", many.len()),
        });
    }
    /// Start the confirmed paste; it runs a slice per frame (perf_frame).
    pub(super) fn perf_files_paste(&mut self) {
        let into = self.place_title(&Place::Folder(self.directory.clone()));
        let job = match &self.device {
            Some(d) => d.start_paste().map(|job| PasteJob::Files { job, into }),
            None => {
                if !self.perf_files_writable() {
                    Err("Factory files are read-only. Choose a user or USB folder.".into())
                } else {
                    Ok(PasteJob::Demo {
                        items: self.clipboard.clone(),
                        done: 0,
                        into,
                    })
                }
            }
        };
        match job {
            Ok(job) => {
                let total = match &job {
                    PasteJob::Files { job, .. } => job.progress().total,
                    PasteJob::Demo { items, .. } => items.len(),
                };
                self.perf.files.job = Some(job);
                self.notify(&format!("Copying 1 of {total}"));
            }
            Err(e) => self.error(&format!("Copy failed: {e}")),
        }
    }
    /// Escape while copying cancels; true when a copy was running.
    pub(super) fn perf_files_cancel_job(&mut self) -> bool {
        let Some(job) = self.perf.files.job.take() else {
            return false;
        };
        let (done, total, result) = match job {
            PasteJob::Files { job, .. } => {
                let total = job.progress().total;
                let result = job.cancel();
                self.refresh_files_listing();
                match result {
                    Ok(done) => (done, total, Ok(())),
                    Err(e) => (0, total, Err(e)),
                }
            }
            PasteJob::Demo { items, done, .. } => (done, items.len(), Ok(())),
        };
        self.sync_device_view();
        self.perf_files_relist();
        match result {
            Ok(()) => self.notify(&format!("Copy cancelled after {done} of {total}")),
            Err(e) => self.error(&format!("Copy cancelled: {e}")),
        }
        true
    }
    /// One frame's share of a running paste. True while one runs.
    pub(super) fn perf_files_step(&mut self) -> bool {
        let slice = self.perf.policy.copy_slice;
        let Some(job) = self.perf.files.job.as_mut() else {
            return false;
        };
        let (progress, into, first) = match job {
            PasteJob::Files { job, into } => {
                let first = job
                    .destinations()
                    .next()
                    .and_then(|p| p.file_name())
                    .map(|n| n.to_string_lossy().into_owned());
                match job.step(slice) {
                    Ok(p) => ((p.done, p.total, p.finished), into.clone(), first),
                    Err(e) => {
                        if let Some(PasteJob::Files { job, .. }) = self.perf.files.job.take() {
                            let _ = job.cancel();
                        }
                        self.refresh_files_listing();
                        self.sync_device_view();
                        self.perf_files_relist();
                        self.error(&format!("Copy failed: {e}"));
                        return false;
                    }
                }
            }
            PasteJob::Demo { items, done, into } => {
                let item = items[*done].clone();
                *done += 1;
                let progress = (*done, items.len(), *done == items.len());
                let into = into.clone();
                let name = self.demo_paste(&item);
                (progress, into, Some(name))
            }
        };
        let (done, total, finished) = progress;
        if finished {
            self.perf.files.job = None;
            match &mut self.device {
                Some(d) => {
                    d.clipboard.clear();
                }
                None => self.clipboard.clear(),
            }
            self.sync_device_view();
            self.refresh_files_listing();
            if let Some(first) = first {
                self.perf_files_focus(&first);
            }
            self.perf_files_relist();
            self.notify(&match total {
                1 => format!("Copied 1 item into {into}"),
                n => format!("Copied {n} items into {into}"),
            });
        } else {
            self.notify(&format!("Copying {} of {total}", done + 1));
        }
        self.navigation_dirty = true;
        !finished
    }
    /// The desktop demo's paste of one virtual item into the Files folder;
    /// returns its new name.
    fn demo_paste(&mut self, source: &str) -> String {
        let folder = source.ends_with('/');
        let path = source.trim_end_matches('/').to_string();
        let name = path.rsplit('/').next().unwrap_or_default().to_string();
        let exists = |n: &str, files: &[String]| files.iter().any(|f| f.trim_end_matches('/') == n);
        let mut dest = name.clone();
        let mut n = 2;
        while exists(&dest, &self.files) {
            dest = format!("Copy {n} of {name}");
            n += 1;
        }
        self.files.push(if folder {
            format!("{dest}/")
        } else {
            dest.clone()
        });
        if folder {
            let target = join(&self.directory, &dest);
            let copies: Vec<(String, Vec<String>)> = self
                .folders
                .iter()
                .filter(|(p, _)| **p == path || p.starts_with(&format!("{path}/")))
                .map(|(p, contents)| (format!("{target}{}", &p[path.len()..]), contents.clone()))
                .collect();
            self.folders.extend(copies);
        }
        dest
    }
    /// Load on a Files row: a loadable file or a patch or performance folder
    /// loads; anything else does nothing.
    pub(super) fn perf_files_load(&mut self) -> bool {
        self.perf_files_relist();
        let Some(row) = self.perf_files_focused() else {
            return true;
        };
        if row.location.is_some() {
            return false;
        }
        let path = self.perf_files_path(&row);
        let lower = row.key.to_lowercase();
        let holds = |kind: &str| {
            self.list_folder(&path).is_ok_and(|names| match kind {
                "perf" => names
                    .iter()
                    .any(|n| n.eq_ignore_ascii_case("performance.grm")),
                _ => {
                    names
                        .iter()
                        .filter(|n| n.to_lowercase().ends_with(".grp"))
                        .count()
                        == 1
                }
            })
        };
        if !row.folder && is_audio(&row.key) {
            self.load_sample_path(&path, &row.key);
        } else if lower.ends_with(".grm") || row.folder && holds("perf") {
            let perf = if row.folder {
                path.clone()
            } else {
                path.rsplit_once('/')
                    .map_or(path.clone(), |(p, _)| p.to_string())
            };
            let name = perf.rsplit('/').next().unwrap_or_default().to_string();
            self.perf_ask(
                vec![format!("Load {name}"), "as the working performance".into()],
                if self.patches.iter().any(|p| p.dirty) {
                    "Unsaved changes to the working performance are lost."
                } else {
                    "The working performance is replaced."
                },
                "Load",
                "Load",
                Verb::LoadPerfPath(perf),
            );
        } else if lower.ends_with(".grp") || row.folder && holds("patch") {
            self.load_patch_path(&path, &row.label);
        }
        true
    }

    /// Per-frame work of the root: preset stencils while a page shows them,
    /// and a running paste. True while it needs the next frame.
    pub(super) fn perf_frame(&mut self) -> bool {
        if self.tab == 4
            && matches!(self.perf.page, PRESETS | PARTS)
            && self.perf.policy.preset_stencils
        {
            let budget = self.perf.policy.stencils_per_frame;
            if let Some(d) = &mut self.device {
                d.fill_stencils(budget);
            }
        }
        self.perf_files_step()
    }
}
