//! ext-workspace-v1 for pagers and bars: one group per monitor, announced on
//! its `wl_output`, and one workspace per Pastel workspace. Synced from dirty
//! frames like the IPC, so an idle desktop sends nothing.
//!
//! Pastel's workspaces are dynamic and named by their position, and the protocol
//! can neither rename a workspace nor move one between groups: a workspace whose
//! position or monitor changes is announced again.

use luminaria::desktop::{
    WorkspaceManager, WorkspaceRequest, WorkspaceRequestKind, workspace_state,
};

use crate::state::State;

struct Group {
    monitor: u64,
    handle: u32,
    /// The output's identity, which still names it after it is gone.
    identity: u64,
}

struct Announced {
    /// Pastel's workspace id.
    id: u64,
    handle: u32,
    group: u32,
    index: usize,
    state: u32,
}

pub struct Workspaces<'d> {
    manager: WorkspaceManager<'d>,
    groups: Vec<Group>,
    workspaces: Vec<Announced>,
}

impl<'d> Workspaces<'d> {
    pub fn new(manager: WorkspaceManager<'d>) -> Self {
        Self {
            manager,
            groups: Vec::new(),
            workspaces: Vec::new(),
        }
    }

    /// The Pastel workspace a protocol handle names.
    fn workspace_of(&self, handle: u32) -> Option<u64> {
        self.workspaces
            .iter()
            .find(|w| w.handle == handle)
            .map(|w| w.id)
    }
}

impl State<'_> {
    /// Brings the protocol's view up to date with the desktop.
    pub(crate) fn sync_workspaces(&mut self) {
        let Some(sync) = self.workspaces.as_mut() else {
            return;
        };
        let (desktop, screens) = (&self.desktop, self.services.screens);
        let mut changed = false;

        // Groups follow monitors.
        let monitors: Vec<u64> = desktop.monitors().iter().map(|m| m.id).collect();
        let gone: Vec<Group> = {
            let (keep, gone) = std::mem::take(&mut sync.groups)
                .into_iter()
                .partition(|g| monitors.contains(&g.monitor));
            sync.groups = keep;
            gone
        };
        for group in gone {
            for workspace in sync.workspaces.iter().filter(|w| w.group == group.handle) {
                sync.manager.remove_workspace(workspace.handle);
            }
            sync.workspaces.retain(|w| w.group != group.handle);
            sync.manager
                .group_remove_output(group.handle, group.identity);
            sync.manager.remove_group(group.handle);
            changed = true;
        }
        for monitor in desktop.monitors() {
            if sync.groups.iter().any(|g| g.monitor == monitor.id) {
                continue;
            }
            let Some(output) = self.outputs.iter().find(|o| o.id().0 == monitor.id) else {
                continue;
            };
            // Until its wl_output exists there is nothing to announce it on;
            // a later frame tries again.
            let identity = screens.output_identity(output);
            if identity == 0 {
                continue;
            }
            let handle = sync.manager.create_group();
            sync.manager.group_add_output(handle, screens, output);
            sync.groups.push(Group {
                monitor: monitor.id,
                handle,
                identity,
            });
            changed = true;
        }

        // Workspaces follow their monitor and position.
        let mut seen = Vec::new();
        for monitor in desktop.monitors() {
            let Some(group) = sync
                .groups
                .iter()
                .find(|g| g.monitor == monitor.id)
                .map(|g| g.handle)
            else {
                continue;
            };
            for (index, workspace) in monitor.workspaces.iter().enumerate() {
                seen.push(workspace.id);
                let state = if index == monitor.active {
                    workspace_state::ACTIVE
                } else {
                    0
                };
                let current = sync.workspaces.iter().position(|w| w.id == workspace.id);
                if let Some(at) = current {
                    let known = &sync.workspaces[at];
                    if known.group == group && known.index == index {
                        if known.state != state {
                            sync.manager.set_state(known.handle, state);
                            sync.workspaces[at].state = state;
                            changed = true;
                        }
                        continue;
                    }
                    sync.manager.remove_workspace(known.handle);
                    sync.workspaces.remove(at);
                }
                let name = (index + 1).to_string();
                let id = format!("pastel-{}", workspace.id);
                let handle = sync
                    .manager
                    .add_workspace(group, &id, &name, &[index as i32]);
                sync.manager.set_state(handle, state);
                sync.workspaces.push(Announced {
                    id: workspace.id,
                    handle,
                    group,
                    index,
                    state,
                });
                changed = true;
            }
        }
        let before = sync.workspaces.len();
        sync.workspaces.retain(|w| {
            let keep = seen.contains(&w.id);
            if !keep {
                sync.manager.remove_workspace(w.handle);
            }
            keep
        });
        changed |= sync.workspaces.len() != before;
        if changed {
            sync.manager.commit();
        }
    }

    /// A pager asked to switch. Workspaces are Pastel's: creating, removing and
    /// moving them is not offered, and deactivating one means nothing here.
    pub(crate) fn workspace_request(&mut self, request: WorkspaceRequest) {
        if request.kind != WorkspaceRequestKind::Activate {
            return;
        }
        let Some(id) = self
            .workspaces
            .as_ref()
            .and_then(|w| w.workspace_of(request.workspace))
        else {
            return;
        };
        if crate::ipc::focus_workspace(&mut self.desktop, id) {
            self.layer_focus = None;
            self.dirty = true;
        }
    }
}
