use crate::config::{SessionConfig, SessionFolder};
use crate::persistence::PersistenceManager;
use egui::Ui;
use egui_ltreeview::{TreeView, NodeBuilder, Action};
use uuid::Uuid;
use std::cell::RefCell;

#[derive(Debug, Clone, PartialEq)]
pub enum SessionManagerAction {
    Connect(Uuid),
    Edit(Uuid),
    Duplicate(Uuid),
    Delete(Uuid),
    NewSession,
    NewSessionInFolder(Uuid),
    NewFolder,
    EditFolder(Uuid),
    DeleteFolder(Uuid),
}

// Node ID wrapper to distinguish folders from sessions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NodeId {
    Folder(Uuid),
    Session(Uuid),
    #[allow(dead_code)]
    Root,
}

pub struct SessionManagerUi {
    filter: String,
}

impl Default for SessionManagerUi {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionManagerUi {
    pub fn new() -> Self {
        Self {
            filter: String::new(),
        }
    }

    pub fn show(&mut self, ui: &mut Ui, persistence: &mut PersistenceManager) -> Option<SessionManagerAction> {
        let mut action = None;
        ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
            ui.set_min_width(0.0);
            // Action buttons
            ui.horizontal_wrapped(|ui| {
                if ui.button("Add session").clicked() {
                    action = Some(SessionManagerAction::NewSession);
                }
                if ui.button("Add group").clicked() {
                    action = Some(SessionManagerAction::NewFolder);
                }
            });
            ui.add_space(4.0);
            // Filter input
            ui.horizontal(|ui| {
                ui.label("Filter:");
                ui.add(
                    egui::TextEdit::singleline(&mut self.filter)
                        .desired_width(f32::INFINITY)
                );
            });
            ui.add_space(4.0);
            // Build tree data
            let tree_id = ui.make_persistent_id("session_tree");
            let context_action: RefCell<Option<SessionManagerAction>> = RefCell::new(None);
            let (_response, actions) = TreeView::new(tree_id)
                .allow_drag_and_drop(false)
                .show(ui, |builder| {
                    self.build_tree(builder, persistence, None, &context_action);
                });
            // Handle context menu actions
            if let Some(ctx_action) = context_action.into_inner() {
                action = Some(ctx_action);
            }
            // Handle tree actions
            for tree_action in actions {
                match tree_action {
                    Action::Activate(activate) => {
                        // Double-click or Enter pressed on selection
                        if let Some(NodeId::Session(id)) = activate.selected.first() {
                            action = Some(SessionManagerAction::Connect(*id));
                        }
                    }
                    _ => {}
                }
            }
        });
        action
    }

    fn build_tree(
        &self,
        builder: &mut egui_ltreeview::TreeViewBuilder<NodeId>,
        persistence: &PersistenceManager,
        parent_id: Option<Uuid>,
        action: &RefCell<Option<SessionManagerAction>>,
    ) {
        // Get folders at this level
        let folders: Vec<SessionFolder> = persistence.child_folders(parent_id)
            .into_iter()
            .cloned()
            .collect();
        // Get sessions at this level (with filter applied)
        let sessions: Vec<SessionConfig> = persistence.sessions_in_folder(parent_id)
            .into_iter()
            .filter(|s| {
                if self.filter.is_empty() {
                    true
                } else {
                    let filter_lower = self.filter.to_lowercase();
                    s.name.to_lowercase().contains(&filter_lower)
                        || s.host.to_lowercase().contains(&filter_lower)
                }
            })
            .cloned()
            .collect();
        // Add folders
        for folder in folders {
            let folder_id = folder.id;
            let folder_name = folder.name.clone();
            builder.node(NodeBuilder::dir(NodeId::Folder(folder_id))
                .label(format!("📁 {}", folder_name))
                .default_open(folder.expanded)
                .flatten(false)
                .context_menu(|ui| {
                    if ui.button("New Session").clicked() {
                        *action.borrow_mut() = Some(SessionManagerAction::NewSessionInFolder(folder_id));
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Rename Group").clicked() {
                        *action.borrow_mut() = Some(SessionManagerAction::EditFolder(folder_id));
                        ui.close();
                    }
                    if ui.button("Delete Group").clicked() {
                        *action.borrow_mut() = Some(SessionManagerAction::DeleteFolder(folder_id));
                        ui.close();
                    }
                }));
            // Recursively add children
            self.build_tree(builder, persistence, Some(folder_id), action);
            builder.close_dir();
        }
        // Add sessions
        for session in sessions {
            let session_id = session.id;
            let session_name = session.name.clone();
            builder.node(NodeBuilder::leaf(NodeId::Session(session_id))
                .label(format!("💻 {}", session_name))
                .context_menu(|ui| {
                    if ui.button("Connect").clicked() {
                        *action.borrow_mut() = Some(SessionManagerAction::Connect(session_id));
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Edit").clicked() {
                        *action.borrow_mut() = Some(SessionManagerAction::Edit(session_id));
                        ui.close();
                    }
                    if ui.button("Duplicate").clicked() {
                        *action.borrow_mut() = Some(SessionManagerAction::Duplicate(session_id));
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Delete").clicked() {
                        *action.borrow_mut() = Some(SessionManagerAction::Delete(session_id));
                        ui.close();
                    }
                }));
        }
    }

    #[allow(dead_code)]
    pub fn selected_session(&self) -> Option<Uuid> {
        None // Selection is now managed by the TreeView widget
    }
}
