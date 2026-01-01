use crate::config::{SessionConfig, SessionFolder};
use crate::persistence::PersistenceManager;
use crate::session_manager::SessionManagerAction;
use egui::Ui;
use std::collections::HashSet;
use uuid::Uuid;

// Tree view layout constants
const ITEM_HEIGHT: f32 = 18.0;
const INDENT_WIDTH: f32 = 16.0;
const DROP_INDICATOR_HEIGHT: f32 = 2.0;
const EXPAND_BUTTON_SIZE: f32 = 14.0;
// Fraction of item height at edges that triggers root-level drop instead of into-folder
const EDGE_DROP_FRACTION: f32 = 0.25;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeItem {
    Folder(Uuid),
    Session(Uuid),
}

impl TreeItem {
    #[allow(dead_code)]
    pub fn is_folder(&self) -> bool {
        matches!(self, TreeItem::Folder(_))
    }
}

#[derive(Debug, Clone, Copy)]
struct DropTarget {
    // Target folder (None = root)
    folder_id: Option<Uuid>,
    // Order position within that folder
    order: u32,
    // Y position for drawing the indicator line
    indicator_y: f32,
    // If dropping directly on a folder to add session to it
    drop_on_folder: Option<Uuid>,
}

#[derive(Debug, Clone)]
struct ItemPosition {
    item: TreeItem,
    // The folder this item belongs to (None = root)
    parent_folder: Option<Uuid>,
    y_min: f32,
    y_max: f32,
}

pub struct SessionTreeView {
    expanded_folders: HashSet<Uuid>,
    dragged_item: Option<TreeItem>,
    drop_target: Option<DropTarget>,
    item_positions: Vec<ItemPosition>,
    initialized: bool,
}

impl Default for SessionTreeView {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionTreeView {
    pub fn new() -> Self {
        Self {
            expanded_folders: HashSet::new(),
            dragged_item: None,
            drop_target: None,
            item_positions: Vec::new(),
            initialized: false,
        }
    }

    pub fn show(
        &mut self,
        ui: &mut Ui,
        persistence: &mut PersistenceManager,
        filter: &str,
    ) -> Option<SessionManagerAction> {
        // Initialize expanded state from persistence on first render
        if !self.initialized {
            for folder in &persistence.folders {
                if folder.expanded {
                    self.expanded_folders.insert(folder.id);
                }
            }
            self.initialized = true;
        }
        self.item_positions.clear();
        self.drop_target = None;
        let mut action = None;
        // Render hierarchy starting from root
        self.render_children(ui, persistence, None, 0, filter, &mut action);
        // Handle drag and drop
        if let Some(dragged) = self.dragged_item {
            if let Some(mouse_pos) = ui.ctx().pointer_latest_pos() {
                self.drop_target = self.calculate_drop_target(persistence, mouse_pos.y, &dragged);
            }
            // Draw drop indicator
            if let Some(target) = &self.drop_target {
                self.draw_drop_indicator(ui, target);
            }
            // Check if drag ended
            if ui.input(|i| i.pointer.any_released()) {
                if let Some(target) = self.drop_target.take() {
                    self.execute_drop(persistence, &dragged, &target);
                }
                self.dragged_item = None;
            }
        }
        action
    }

    fn render_children(
        &mut self,
        ui: &mut Ui,
        persistence: &mut PersistenceManager,
        parent_id: Option<Uuid>,
        depth: usize,
        filter: &str,
        action: &mut Option<SessionManagerAction>,
    ) {
        // Render folders at this level
        let folders: Vec<SessionFolder> = persistence.child_folders(parent_id)
            .iter()
            .cloned()
            .cloned()
            .collect();
        for folder in folders {
            let folder_id = folder.id;
            let y_before = ui.cursor().top();
            if let Some(folder_action) = self.render_folder(ui, &folder, persistence, depth) {
                *action = Some(folder_action);
            }
            let y_after = ui.cursor().top();
            self.item_positions.push(ItemPosition {
                item: TreeItem::Folder(folder_id),
                parent_folder: parent_id,
                y_min: y_before,
                y_max: y_after,
            });
            // Render children if expanded
            if self.expanded_folders.contains(&folder_id) {
                self.render_children(ui, persistence, Some(folder_id), depth + 1, filter, action);
            }
        }
        // Render sessions in this folder
        let sessions: Vec<SessionConfig> = persistence.sessions_in_folder(parent_id)
            .iter()
            .filter(|s| {
                if filter.is_empty() {
                    true
                } else {
                    let filter_lower = filter.to_lowercase();
                    s.name.to_lowercase().contains(&filter_lower)
                        || s.host.to_lowercase().contains(&filter_lower)
                }
            })
            .cloned()
            .cloned()
            .collect();
        for session in sessions {
            let session_id = session.id;
            let y_before = ui.cursor().top();
            if let Some(session_action) = self.render_session(ui, &session, depth) {
                *action = Some(session_action);
            }
            let y_after = ui.cursor().top();
            self.item_positions.push(ItemPosition {
                item: TreeItem::Session(session_id),
                parent_folder: parent_id,
                y_min: y_before,
                y_max: y_after,
            });
        }
    }

    fn render_folder(
        &mut self,
        ui: &mut Ui,
        folder: &SessionFolder,
        persistence: &mut PersistenceManager,
        depth: usize,
    ) -> Option<SessionManagerAction> {
        let mut action = None;
        let folder_id = folder.id;
        let is_expanded = self.expanded_folders.contains(&folder_id);
        let indent = depth as f32 * INDENT_WIDTH;
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), ITEM_HEIGHT),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.set_min_height(ITEM_HEIGHT);
                ui.add_space(indent);
                // Expand/collapse button
                let expand_text = if is_expanded { "▼" } else { "▶" };
                let expand_btn = ui.add_sized(
                    [EXPAND_BUTTON_SIZE, EXPAND_BUTTON_SIZE],
                    egui::Button::new(expand_text).frame(false)
                );
                if expand_btn.clicked() {
                    if is_expanded {
                        self.expanded_folders.remove(&folder_id);
                    } else {
                        self.expanded_folders.insert(folder_id);
                    }
                    persistence.set_folder_expanded(folder_id, !is_expanded);
                }
                // Folder item
                let item_response = ui.add(
                    egui::Button::new(format!("📁 {}", folder.name))
                        .frame(false)
                        .sense(egui::Sense::click_and_drag())
                );
                if item_response.drag_started() {
                    self.dragged_item = Some(TreeItem::Folder(folder_id));
                }
                // Context menu
                item_response.context_menu(|ui| {
                    if ui.button("New Session").clicked() {
                        action = Some(SessionManagerAction::NewSessionInFolder(folder_id));
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Rename Group").clicked() {
                        action = Some(SessionManagerAction::EditFolder(folder_id));
                        ui.close();
                    }
                    if ui.button("Delete Group").clicked() {
                        action = Some(SessionManagerAction::DeleteFolder(folder_id));
                        ui.close();
                    }
                });
            },
        );
        action
    }

    fn render_session(
        &mut self,
        ui: &mut Ui,
        session: &SessionConfig,
        depth: usize,
    ) -> Option<SessionManagerAction> {
        let mut action = None;
        let session_id = session.id;
        let indent = depth as f32 * INDENT_WIDTH;
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), ITEM_HEIGHT),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.set_min_height(ITEM_HEIGHT);
                ui.add_space(indent);
                // Only add expand button space for sessions inside folders (not root)
                if depth > 0 {
                    ui.add_space(EXPAND_BUTTON_SIZE);
                }
                // Session item
                let item_response = ui.add(
                    egui::Button::new(format!("💻 {}", session.name))
                        .frame(false)
                        .sense(egui::Sense::click_and_drag())
                );
                if item_response.double_clicked() {
                    action = Some(SessionManagerAction::Connect(session_id));
                }
                if item_response.drag_started() {
                    self.dragged_item = Some(TreeItem::Session(session_id));
                }
                // Context menu
                item_response.context_menu(|ui| {
                    if ui.button("Connect").clicked() {
                        action = Some(SessionManagerAction::Connect(session_id));
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Edit").clicked() {
                        action = Some(SessionManagerAction::Edit(session_id));
                        ui.close();
                    }
                    if ui.button("Duplicate").clicked() {
                        action = Some(SessionManagerAction::Duplicate(session_id));
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Delete").clicked() {
                        action = Some(SessionManagerAction::Delete(session_id));
                        ui.close();
                    }
                });
            },
        );
        action
    }

    fn calculate_drop_target(
        &self,
        persistence: &PersistenceManager,
        mouse_y: f32,
        dragged_item: &TreeItem,
    ) -> Option<DropTarget> {
        if self.item_positions.is_empty() {
            return None;
        }
        // Find which item the mouse is over
        for pos in &self.item_positions {
            if mouse_y < pos.y_min || mouse_y >= pos.y_max {
                continue;
            }
            let item_mid = (pos.y_min + pos.y_max) / 2.0;
            let in_upper_half = mouse_y < item_mid;
            match (&pos.item, dragged_item) {
                // Dragging a folder
                (TreeItem::Folder(target_id), TreeItem::Folder(dragged_id)) => {
                    if target_id == dragged_id {
                        return None; // Can't drop on self
                    }
                    // Folders can only be reordered at root level
                    if pos.parent_folder.is_some() {
                        return None; // Can't drop folder inside another folder
                    }
                    let order = self.get_folder_order(persistence, *target_id)?;
                    if in_upper_half {
                        return Some(DropTarget {
                            folder_id: None,
                            order,
                            indicator_y: pos.y_min,
                            drop_on_folder: None,
                        });
                    } else {
                        return Some(DropTarget {
                            folder_id: None,
                            order: order + 1,
                            indicator_y: pos.y_max,
                            drop_on_folder: None,
                        });
                    }
                }
                // Dragging a folder over a session
                (TreeItem::Session(_), TreeItem::Folder(_)) => {
                    // Folders can only be reordered among other root folders
                    // Can't drop folder on/between sessions
                    return None;
                }
                // Dragging a session over a folder
                (TreeItem::Folder(target_folder_id), TreeItem::Session(dragged_session_id)) => {
                    // Check if this folder is at root level
                    if pos.parent_folder.is_some() {
                        return None; // Can't interact with nested folders
                    }
                    let item_height = pos.y_max - pos.y_min;
                    let edge_size = item_height * EDGE_DROP_FRACTION;
                    let is_expanded = self.expanded_folders.contains(target_folder_id);
                    // Top edge: drop at root BEFORE this folder
                    if mouse_y < pos.y_min + edge_size {
                        let folder_order = self.get_folder_order(persistence, *target_folder_id)?;
                        // Sessions at root use session orders, not folder orders
                        // We need to find the right position in root sessions
                        return Some(DropTarget {
                            folder_id: None,
                            order: self.calculate_root_session_order_before_folder(persistence, folder_order),
                            indicator_y: pos.y_min,
                            drop_on_folder: None,
                        });
                    }
                    // Bottom edge: drop at root AFTER this folder (only if collapsed)
                    if !is_expanded && mouse_y > pos.y_max - edge_size {
                        let folder_order = self.get_folder_order(persistence, *target_folder_id)?;
                        return Some(DropTarget {
                            folder_id: None,
                            order: self.calculate_root_session_order_after_folder(persistence, folder_order),
                            indicator_y: pos.y_max,
                            drop_on_folder: None,
                        });
                    }
                    // Middle: drop INTO the folder
                    if let Some(session) = persistence.get_session(*dragged_session_id) {
                        if session.folder_id == Some(*target_folder_id) {
                            return None; // Already in this folder
                        }
                    }
                    return Some(DropTarget {
                        folder_id: Some(*target_folder_id),
                        order: persistence.get_last_order_in_folder(Some(*target_folder_id)) + 1,
                        indicator_y: pos.y_min, // Not used for folder highlight
                        drop_on_folder: Some(*target_folder_id),
                    });
                }
                // Dragging a session over another session
                (TreeItem::Session(target_id), TreeItem::Session(dragged_id)) => {
                    if target_id == dragged_id {
                        return None; // Can't drop on self
                    }
                    let target_folder = pos.parent_folder;
                    let order = self.get_session_order(persistence, *target_id)?;
                    if in_upper_half {
                        return Some(DropTarget {
                            folder_id: target_folder,
                            order,
                            indicator_y: pos.y_min,
                            drop_on_folder: None,
                        });
                    } else {
                        return Some(DropTarget {
                            folder_id: target_folder,
                            order: order + 1,
                            indicator_y: pos.y_max,
                            drop_on_folder: None,
                        });
                    }
                }
            }
        }
        // Mouse is below all items
        if let Some(last_pos) = self.item_positions.last() {
            if mouse_y >= last_pos.y_max {
                match dragged_item {
                    TreeItem::Folder(_) => {
                        // Add folder at end of root
                        return Some(DropTarget {
                            folder_id: None,
                            order: persistence.get_last_folder_order(None) + 1,
                            indicator_y: last_pos.y_max,
                            drop_on_folder: None,
                        });
                    }
                    TreeItem::Session(_) => {
                        // Add session at end of wherever the last item was
                        return Some(DropTarget {
                            folder_id: last_pos.parent_folder,
                            order: persistence.get_last_order_in_folder(last_pos.parent_folder) + 1,
                            indicator_y: last_pos.y_max,
                            drop_on_folder: None,
                        });
                    }
                }
            }
        }
        None
    }

    fn get_folder_order(&self, persistence: &PersistenceManager, folder_id: Uuid) -> Option<u32> {
        persistence.get_folder(folder_id).map(|f| f.order)
    }

    fn get_session_order(&self, persistence: &PersistenceManager, session_id: Uuid) -> Option<u32> {
        persistence.get_session(session_id).map(|s| s.order)
    }

    fn calculate_root_session_order_before_folder(&self, _persistence: &PersistenceManager, _folder_order: u32) -> u32 {
        // Sessions at root render after all folders, so "before folder" visually means
        // at the start of root sessions. Use order 1 to insert at beginning.
        1
    }

    fn calculate_root_session_order_after_folder(&self, persistence: &PersistenceManager, _folder_order: u32) -> u32 {
        // For dropping at root "after" a folder - still goes into the session list.
        // Place at the end of root sessions.
        persistence.get_last_order_in_folder(None) + 1
    }

    fn draw_drop_indicator(&self, ui: &mut Ui, target: &DropTarget) {
        let painter = ui.painter();
        if let Some(folder_id) = target.drop_on_folder {
            // Highlight folder row
            if let Some(pos) = self.item_positions.iter().find(|p| p.item == TreeItem::Folder(folder_id)) {
                let rect = egui::Rect::from_min_max(
                    egui::pos2(ui.clip_rect().left(), pos.y_min),
                    egui::pos2(ui.clip_rect().right(), pos.y_max),
                );
                painter.rect_filled(
                    rect,
                    0.0,
                    ui.style().visuals.selection.bg_fill.linear_multiply(0.3),
                );
            }
        } else {
            // Draw horizontal line at indicator_y
            // Indent if dropping inside a folder
            let left_indent = if target.folder_id.is_some() {
                INDENT_WIDTH + EXPAND_BUTTON_SIZE
            } else {
                0.0
            };
            let stroke_color = ui.style().visuals.selection.stroke.color;
            let rect = egui::Rect::from_min_max(
                egui::pos2(ui.clip_rect().left() + left_indent, target.indicator_y - DROP_INDICATOR_HEIGHT / 2.0),
                egui::pos2(ui.clip_rect().right(), target.indicator_y + DROP_INDICATOR_HEIGHT / 2.0),
            );
            painter.rect_filled(rect, 0.0, stroke_color);
        }
    }

    fn execute_drop(&self, persistence: &mut PersistenceManager, dragged: &TreeItem, target: &DropTarget) {
        match dragged {
            TreeItem::Session(session_id) => {
                persistence.move_session(*session_id, target.folder_id, target.order);
                let _ = persistence.save();
            }
            TreeItem::Folder(folder_id) => {
                // Folders always stay at root (parent_id = None)
                persistence.move_folder(*folder_id, None, target.order);
                let _ = persistence.save();
            }
        }
    }
}
