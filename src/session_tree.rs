#![allow(dead_code)]

use crate::config::{SessionConfig, SessionFolder};
use crate::persistence::PersistenceManager;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub enum TreeItem {
    Folder(SessionFolder),
    Session(SessionConfig),
}

impl TreeItem {
    pub fn id(&self) -> Uuid {
        match self {
            TreeItem::Folder(f) => f.id,
            TreeItem::Session(s) => s.id,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            TreeItem::Folder(f) => &f.name,
            TreeItem::Session(s) => &s.name,
        }
    }

    pub fn is_folder(&self) -> bool {
        matches!(self, TreeItem::Folder(_))
    }

    pub fn parent_id(&self) -> Option<Uuid> {
        match self {
            TreeItem::Folder(f) => f.parent_id,
            TreeItem::Session(s) => s.folder_id,
        }
    }
}

pub struct SessionTree {
    items: Vec<(TreeItem, usize)>, // Item and depth
}

impl SessionTree {
    pub fn new() -> Self {
        Self { items: Vec::new() }
    }

    pub fn build(&mut self, persistence: &PersistenceManager) {
        self.items.clear();
        self.build_recursive(persistence, None, 0);
    }

    fn build_recursive(&mut self, persistence: &PersistenceManager, parent_id: Option<Uuid>, depth: usize) {
        // Add folders at this level
        for folder in persistence.child_folders(parent_id) {
            self.items.push((TreeItem::Folder(folder.clone()), depth));
            if folder.expanded {
                self.build_recursive(persistence, Some(folder.id), depth + 1);
            }
        }
        // Add sessions at this level
        for session in persistence.sessions_in_folder(parent_id) {
            self.items.push((TreeItem::Session(session.clone()), depth));
        }
    }

    pub fn items(&self) -> &[(TreeItem, usize)] {
        &self.items
    }

    pub fn find_item(&self, id: Uuid) -> Option<&TreeItem> {
        self.items.iter().find(|(item, _)| item.id() == id).map(|(item, _)| item)
    }

    pub fn get_depth(&self, id: Uuid) -> Option<usize> {
        self.items.iter().find(|(item, _)| item.id() == id).map(|(_, depth)| *depth)
    }
}

impl Default for SessionTree {
    fn default() -> Self {
        Self::new()
    }
}

