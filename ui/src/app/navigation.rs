//! Select navigates one screen context. Focus is identified by action, never draw index.
use super::*;

impl App {
    pub(super) fn focus_context(&self) -> String {
        if self.tab == 4 {
            return self.perf_context();
        }
        if self.sample_root_active() && !self.overlay_active() {
            return self.sample_context();
        }
        if self.in_patch_root() {
            return self.patch_context();
        }
        if self.panel_sidebar && self.modal.is_none() {
            return format!("sidebar:{}", self.menu_root);
        }
        let layer = match &self.modal {
            Some(Modal::Edit { index, .. }) => format!("edit{index}"),
            Some(Modal::Slots { operation, bank }) => format!("slots{operation}{bank}"),
            Some(Modal::Confirm(op)) => format!("confirm{op}"),
            None if self.list_action_origin.is_some() => "item-actions".into(),
            None => "page".into(),
        };
        format!(
            "{}:{}:{}:{}:{layer}",
            self.tab, self.group, self.page, self.source
        )
    }
    pub(super) fn remember_focus(&mut self) -> String {
        let context = self.focus_context();
        if let Some(a) = self.focus_target.clone().or_else(|| {
            self.focus
                .and_then(|i| self.hits.get(i))
                .map(|h| h.a.clone())
        }) {
            self.focus_memory.insert(context.clone(), a);
        }
        context
    }
    pub(super) fn restore_focus(&mut self, before: &str) {
        let context = self.focus_context();
        if context != before {
            self.focus_target = self.focus_memory.get(&context).cloned();
            self.focus = None;
        }
    }
    fn eligible_hits(&self) -> Vec<usize> {
        self.hits
            .iter()
            .enumerate()
            .filter(|(_, h)| {
                if !self.panel_mode || self.modal.is_some() {
                    true
                } else if self.tab == 0 {
                    false
                } else if self.panel_sidebar {
                    matches!(h.a, Action::OpenMenu(..))
                } else {
                    h.r.1 >= 64
                        && !matches!(
                            h.a,
                            Action::Nav(_)
                                | Action::Group(_)
                                | Action::OpenMenu(..)
                                | Action::Nudge(_)
                                | Action::UndoLive
                                | Action::FinishLive
                                | Action::LiveSet(_)
                        )
                }
            })
            .filter(|(i, _)| {
                self.modal.is_some()
                    || self.list_action_origin.is_none()
                    || !self.list_hits.iter().any(|(hit, _)| hit == i)
            })
            .filter(|(_, h)| {
                if !self.panel_mode || self.modal.is_some() || self.panel_sidebar {
                    return true;
                }
                match self.list_action_origin {
                    Some(Action::Part(part)) => {
                        matches!(h.a, Action::PartEnable(i) | Action::EditVolume(i) if i == part)
                    }
                    Some(Action::Preset(_)) => matches!(h.a, Action::Op(_)),
                    _ => true,
                }
            })
            .map(|(i, _)| i)
            .collect()
    }
    pub(super) fn finish_focus(&mut self, p: &mut Painter) {
        self.navigation_dirty = false;
        let context = self.focus_context();
        if self.rendered_context != context {
            self.focus = None;
            self.focus_target = match self.modal {
                Some(Modal::Edit { value, .. }) => Some(Action::Choose(value)),
                _ => self.focus_memory.get(&context).cloned(),
            };
            self.rendered_context = context;
        }
        let candidates = self.eligible_hits();
        if let Some(target) = &self.focus_target {
            self.focus = candidates
                .iter()
                .copied()
                .find(|i| self.hits[*i].a == *target);
        }
        if self.focus.is_none() && self.panel_mode && (self.tab != 0 || self.modal.is_some()) {
            self.focus = if let Some(Modal::Edit { value, .. }) = self.modal {
                candidates
                    .iter()
                    .copied()
                    .find(|i| self.hits[*i].a == Action::Choose(value))
                    .or_else(|| candidates.first().copied())
            } else if self.list_action_origin.is_some() && self.modal.is_none() {
                let primary = match self.list_action_origin {
                    Some(Action::Part(i)) => Action::PartEnable(i),
                    Some(Action::Preset(_)) => Action::Op("load_patch"),
                    _ => Action::Op(match self.tab {
                        4 => "load_perf",
                        5 => "rename_file",
                        _ => "load_sample",
                    }),
                };
                candidates
                    .iter()
                    .copied()
                    .find(|i| self.hits[*i].a == primary)
            } else if self.modal.is_none() {
                candidates
                    .iter()
                    .copied()
                    .find(|i| self.hits[*i].a == Action::Section(self.page))
            } else {
                None
            }
            .or_else(|| candidates.first().copied());
        }
        if let Some(hit) = self.focus.and_then(|i| self.hits.get(i)) {
            self.focus_target = Some(hit.a.clone());
            if let (Some(list), Some((_, row))) = (
                self.list,
                self.list_hits.iter().find(|(i, _)| Some(*i) == self.focus),
            ) {
                if self.modal.is_none() && row.3 <= list.bounds.3 {
                    let shift = if row.1 < list.bounds.1 {
                        row.1 - list.bounds.1
                    } else {
                        (row.1 + row.3 - list.bounds.1 - list.bounds.3).max(0)
                    };
                    let offset = (list.offset + shift).clamp(0, list.max_offset());
                    if offset != list.offset {
                        self.scroll_offsets.insert(self.scroll_key(), offset);
                        self.navigation_dirty = true;
                        return;
                    }
                }
            }
            let r = hit.r;
            if r.2 > 4 && r.3 > 4 {
                p.border(Rect(r.0 + 1, r.1 + 1, r.2 - 2, r.3 - 2), AMBER);
                p.border(Rect(r.0 + 2, r.1 + 2, r.2 - 4, r.3 - 4), AMBER);
            }
        }
    }
    pub(super) fn navigate_focus(&mut self, delta: i32) {
        let candidates = self.eligible_hits();
        if candidates.is_empty() || delta == 0 {
            return;
        }
        let current = self
            .focus_target
            .as_ref()
            .and_then(|a| candidates.iter().position(|i| self.hits[*i].a == *a))
            .or_else(|| {
                self.focus
                    .and_then(|i| candidates.iter().position(|n| *n == i))
            });
        let next = match current {
            Some(i) => (i as i32 + delta).clamp(0, candidates.len() as i32 - 1) as usize,
            None => {
                if delta < 0 {
                    candidates.len() - 1
                } else {
                    0
                }
            }
        };
        self.focus_hit(candidates[next]);
    }
    fn focus_hit(&mut self, index: usize) {
        if self.panel_sidebar && self.modal.is_none() {
            if let Action::OpenMenu(tab, group) = self.hits[index].a {
                self.tab = tab;
                self.group = group;
                self.page = 0;
            }
        } else if self.modal.is_none() {
            if let (Some(list), Some((_, r))) =
                (self.list, self.list_hits.iter().find(|(i, _)| *i == index))
            {
                let shift = if r.1 < list.bounds.1 {
                    r.1 - list.bounds.1
                } else {
                    (r.1 + r.3 - list.bounds.1 - list.bounds.3).max(0)
                };
                let offset = (list.offset + shift).clamp(0, list.max_offset());
                self.scroll_offsets.insert(self.scroll_key(), offset);
            }
        }
        self.focus = Some(index);
        self.focus_target = Some(self.hits[index].a.clone());
        self.navigation_dirty = true;
    }
    pub(super) fn open_panel_menu(&mut self, root: usize) {
        if root == 2 {
            // Shift+Perf opens the Perf root (Perf).
            self.open_perf_root(false);
            return;
        }
        let before = self.remember_focus();
        self.menu_root = root;
        self.page_return = None;
        self.tab = match root {
            0 => 1,
            1 => 3,
            _ => 5,
        };
        self.group = 0;
        self.page = 0;
        self.live = None;
        self.modal = None;
        self.panel_sidebar = true;
        self.list_action_origin = None;
        if let Some(Action::OpenMenu(tab, group)) =
            self.focus_memory.get(&format!("sidebar:{root}"))
        {
            self.tab = *tab;
            self.group = *group;
        }
        self.restore_focus(&before);
        self.device_context();
    }
    pub(super) fn panel_turn(&mut self, delta: i32) {
        if delta == 0 {
            return;
        }
        if self.modal.is_none() && self.live.is_some() {
            self.action(Action::Nudge(delta as f32));
        } else if let Some(Modal::Edit { index, .. }) = self.modal {
            if let Kind::Number { step, .. } = self.catalog[index].kind {
                self.action(Action::Adjust(delta as f32 * step));
            } else {
                self.move_focus(delta);
            }
        } else if self.tab != 0 || self.modal.is_some() || !self.panel_mode {
            self.move_focus(delta);
        }
        // In the main native screen, the engine owns last-control fine adjustment and part focus.
    }
    pub(super) fn panel_press(&mut self) {
        if matches!(self.modal, Some(Modal::Edit { index, .. }) if matches!(self.catalog[index].kind, Kind::Number { .. }))
        {
            self.action(Action::Apply);
        } else if self.live.is_some() && self.modal.is_none() {
            self.action(Action::FinishLive);
        } else if let Some(hit) = self.focus.and_then(|i| self.hits.get(i)).cloned() {
            let enter_actions = self.panel_mode
                && self.modal.is_none()
                && self.list_action_origin.is_none()
                && matches!(
                    hit.a,
                    Action::Select(_) | Action::Preset(_) | Action::Part(_)
                );
            self.action(hit.a.clone());
            if enter_actions {
                self.list_action_origin = Some(hit.a);
                self.navigation_dirty = true;
                // Each new selection starts at its primary action, even if the
                // previous visit ended at a different operation.
                self.focus_memory.remove(&self.focus_context());
            }
        } else if matches!(self.modal, Some(Modal::Edit { .. })) {
            self.action(Action::Apply);
        } else if self.tab == 0 && self.get("sampling") != 0. {
            self.operation("record");
        }
    }
    pub(super) fn panel_slider(&mut self, value: u8) {
        if let Some(edit) = self.live {
            let (lo, hi, step) = self.target_range(edit.target);
            self.write_live(((lo + (hi - lo) * value as f32 / 255.) / step).round() * step);
        } else {
            let candidates = self.eligible_hits();
            if !candidates.is_empty() {
                self.focus_hit(candidates[(candidates.len() - 1) * value as usize / 255]);
            }
        }
    }
}
