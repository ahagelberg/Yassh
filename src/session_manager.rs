use crate::persistence::PersistenceManager;
use crate::session_tree_view::SessionTreeView;
use egui::Ui;
use uuid::Uuid;

// Spacing constants
const BUTTON_SPACING: f32 = 4.0;

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

pub struct SessionManagerUi {
    filter: String,
    tree_view: SessionTreeView,
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
            tree_view: SessionTreeView::new(),
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
            ui.add_space(BUTTON_SPACING);
            // Filter input
            ui.horizontal(|ui| {
                ui.label("Filter:");
                ui.add(
                    egui::TextEdit::singleline(&mut self.filter)
                        .desired_width(f32::INFINITY)
                );
            });
            ui.add_space(BUTTON_SPACING);
            // Show custom tree view
            if let Some(tree_action) = self.tree_view.show(ui, persistence, &self.filter) {
                action = Some(tree_action);
            }
        });
        action
    }

    #[allow(dead_code)]
    pub fn selected_session(&self) -> Option<Uuid> {
        None
    }
}
