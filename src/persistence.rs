use crate::config::{AppConfig, SessionConfig, SessionFolder};
use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;
use uuid::Uuid;

const APP_CONFIG_FILE: &str = "config.json";
const SESSIONS_FILE: &str = "sessions.json";
const FOLDERS_FILE: &str = "folders.json";
const OPEN_SESSIONS_FILE: &str = "open_sessions.json";

fn get_config_dir() -> Result<PathBuf> {
    let config_dir = dirs::config_dir()
        .context("Could not find config directory")?
        .join("Yassh");
    if !config_dir.exists() {
        fs::create_dir_all(&config_dir)?;
    }
    Ok(config_dir)
}

pub fn load_app_config() -> AppConfig {
    let path = match get_config_dir() {
        Ok(dir) => dir.join(APP_CONFIG_FILE),
        Err(e) => {
            log::error!("Failed to get config directory: {}. Using default config.", e);
            return AppConfig::default();
        }
    };
    if !path.exists() {
        return AppConfig::default();
    }
    let content = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => {
            log::error!("Failed to read app config file: {}. Using default config.", e);
            return AppConfig::default();
        }
    };
    match serde_json::from_str(&content) {
        Ok(config) => config,
        Err(e) => {
            log::error!("Failed to parse app config: {}. Using default config.", e);
            AppConfig::default()
        }
    }
}

pub fn save_app_config(config: &AppConfig) -> Result<()> {
    let path = get_config_dir()?.join(APP_CONFIG_FILE);
    let content = serde_json::to_string_pretty(config)?;
    fs::write(&path, content)?;
    Ok(())
}

pub fn load_sessions() -> Result<Vec<SessionConfig>> {
    let path = get_config_dir()?.join(SESSIONS_FILE);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(&path)?;
    serde_json::from_str(&content).context("Failed to parse sessions")
}

pub fn save_sessions(sessions: &[SessionConfig]) -> Result<()> {
    let path = get_config_dir()?.join(SESSIONS_FILE);
    let content = serde_json::to_string_pretty(sessions)?;
    fs::write(&path, content)?;
    Ok(())
}

pub fn load_folders() -> Result<Vec<SessionFolder>> {
    let path = get_config_dir()?.join(FOLDERS_FILE);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(&path)?;
    serde_json::from_str(&content).context("Failed to parse folders")
}

pub fn save_folders(folders: &[SessionFolder]) -> Result<()> {
    let path = get_config_dir()?.join(FOLDERS_FILE);
    let content = serde_json::to_string_pretty(folders)?;
    fs::write(&path, content)?;
    Ok(())
}

pub fn load_open_sessions() -> Result<Vec<Uuid>> {
    let path = get_config_dir()?.join(OPEN_SESSIONS_FILE);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(&path)?;
    serde_json::from_str(&content).context("Failed to parse open sessions")
}

pub fn save_open_sessions(session_ids: &[Uuid]) -> Result<()> {
    let path = get_config_dir()?.join(OPEN_SESSIONS_FILE);
    let content = serde_json::to_string_pretty(session_ids)?;
    fs::write(&path, content)?;
    Ok(())
}

#[derive(Default)]
pub struct PersistenceManager {
    pub sessions: Vec<SessionConfig>,
    pub folders: Vec<SessionFolder>,
}

impl PersistenceManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn load(&mut self) -> Result<()> {
        self.sessions = match load_sessions() {
            Ok(sessions) => sessions,
            Err(e) => {
                log::error!("Failed to load sessions: {}. Starting with empty session list.", e);
                Vec::new()
            }
        };
        self.folders = match load_folders() {
            Ok(folders) => folders,
            Err(e) => {
                log::error!("Failed to load folders: {}. Starting with empty folder list.", e);
                Vec::new()
            }
        };
        self.assign_missing_folder_orders();
        Ok(())
    }

    fn assign_missing_folder_orders(&mut self) {
        // Check if any folders have order 0 (unassigned)
        let needs_migration = self.folders.iter().all(|f| f.order == 0) && !self.folders.is_empty();
        if !needs_migration {
            return;
        }
        // Assign orders based on current position in vector (insertion order)
        for (index, folder) in self.folders.iter_mut().enumerate() {
            folder.order = (index + 1) as u32;
        }
    }

    pub fn save(&self) -> Result<()> {
        save_sessions(&self.sessions)?;
        save_folders(&self.folders)?;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn sessions(&self) -> &[SessionConfig] {
        &self.sessions
    }

    #[allow(dead_code)]
    pub fn folders(&self) -> &[SessionFolder] {
        &self.folders
    }

    pub fn add_session(&mut self, mut session: SessionConfig) {
        // Assign order to be at the end of the folder
        let max_order = self.sessions
            .iter()
            .filter(|s| s.folder_id == session.folder_id)
            .map(|s| s.order)
            .max()
            .unwrap_or(0);
        session.order = max_order + 1;
        self.sessions.push(session);
    }

    pub fn update_session(&mut self, session: SessionConfig) {
        if let Some(existing) = self.sessions.iter_mut().find(|s| s.id == session.id) {
            *existing = session;
        }
    }

    pub fn remove_session(&mut self, id: Uuid) {
        self.sessions.retain(|s| s.id != id);
    }

    pub fn get_session(&self, id: Uuid) -> Option<&SessionConfig> {
        self.sessions.iter().find(|s| s.id == id)
    }

    pub fn add_folder(&mut self, mut folder: SessionFolder) {
        let max_order = self.get_last_folder_order(folder.parent_id);
        folder.order = max_order + 1;
        self.folders.push(folder);
    }

    #[allow(dead_code)]
    pub fn update_folder(&mut self, folder: SessionFolder) {
        if let Some(existing) = self.folders.iter_mut().find(|f| f.id == folder.id) {
            *existing = folder;
        }
    }

    pub fn remove_folder(&mut self, id: Uuid) {
        self.folders.retain(|f| f.id != id);
        for session in &mut self.sessions {
            if session.folder_id == Some(id) {
                session.folder_id = None;
            }
        }
    }

    pub fn get_folder(&self, id: Uuid) -> Option<&SessionFolder> {
        self.folders.iter().find(|f| f.id == id)
    }

    pub fn sessions_in_folder(&self, folder_id: Option<Uuid>) -> Vec<&SessionConfig> {
        let mut sessions: Vec<&SessionConfig> = self.sessions
            .iter()
            .filter(|s| s.folder_id == folder_id)
            .collect();
        sessions.sort_by_key(|s| s.order);
        sessions
    }

    pub fn child_folders(&self, parent_id: Option<Uuid>) -> Vec<&SessionFolder> {
        let mut folders: Vec<&SessionFolder> = self.folders
            .iter()
            .filter(|f| f.parent_id == parent_id)
            .collect();
        folders.sort_by_key(|f| f.order);
        folders
    }

    pub fn duplicate_session(&mut self, id: Uuid) -> Option<Uuid> {
        let session = self.get_session(id)?.clone();
        let mut new_session = session;
        new_session.id = Uuid::new_v4();
        new_session.name = format!("{} (Copy)", new_session.name);
        let new_id = new_session.id;
        self.add_session(new_session);
        Some(new_id)
    }

    pub fn get_last_order_in_folder(&self, folder_id: Option<Uuid>) -> u32 {
        self.sessions
            .iter()
            .filter(|s| s.folder_id == folder_id)
            .map(|s| s.order)
            .max()
            .unwrap_or(0)
    }

    pub fn get_last_folder_order(&self, parent_id: Option<Uuid>) -> u32 {
        self.folders
            .iter()
            .filter(|f| f.parent_id == parent_id)
            .map(|f| f.order)
            .max()
            .unwrap_or(0)
    }

    pub fn move_session(&mut self, session_id: Uuid, target_folder_id: Option<Uuid>, target_order: u32) {
        let source_folder_id = self.sessions.iter().find(|s| s.id == session_id).map(|s| s.folder_id);
        let source_folder_id = match source_folder_id {
            Some(fid) => fid,
            None => return,
        };
        // Shift existing items at or after target_order to make room
        for session in &mut self.sessions {
            if session.id != session_id && session.folder_id == target_folder_id && session.order >= target_order {
                session.order += 1;
            }
        }
        // Now set the moved session's folder and order
        if let Some(session) = self.sessions.iter_mut().find(|s| s.id == session_id) {
            session.folder_id = target_folder_id;
            session.order = target_order;
        }
        // Normalize orders in source folder (if different from target)
        if source_folder_id != target_folder_id {
            self.normalize_session_orders(source_folder_id);
        }
        self.normalize_session_orders(target_folder_id);
    }

    pub fn move_folder(&mut self, folder_id: Uuid, target_parent_id: Option<Uuid>, target_order: u32) {
        // Prevent moving folder into itself
        if Some(folder_id) == target_parent_id {
            return;
        }
        let source_parent_id = self.folders.iter().find(|f| f.id == folder_id).map(|f| f.parent_id);
        let source_parent_id = match source_parent_id {
            Some(pid) => pid,
            None => return,
        };
        // Shift existing folders at or after target_order to make room
        for folder in &mut self.folders {
            if folder.id != folder_id && folder.parent_id == target_parent_id && folder.order >= target_order {
                folder.order += 1;
            }
        }
        // Now set the moved folder's parent and order
        if let Some(folder) = self.folders.iter_mut().find(|f| f.id == folder_id) {
            folder.parent_id = target_parent_id;
            folder.order = target_order;
        }
        // Normalize orders in source parent (if different from target)
        if source_parent_id != target_parent_id {
            self.normalize_folder_orders(source_parent_id);
        }
        self.normalize_folder_orders(target_parent_id);
    }

    pub fn normalize_session_orders(&mut self, folder_id: Option<Uuid>) {
        let mut sessions: Vec<(Uuid, u32)> = self.sessions
            .iter()
            .filter(|s| s.folder_id == folder_id)
            .map(|s| (s.id, s.order))
            .collect();
        sessions.sort_by_key(|(_, order)| *order);
        for (new_order, (id, _)) in sessions.into_iter().enumerate() {
            if let Some(session) = self.sessions.iter_mut().find(|s| s.id == id) {
                session.order = (new_order + 1) as u32;
            }
        }
    }

    pub fn normalize_folder_orders(&mut self, parent_id: Option<Uuid>) {
        let mut folders: Vec<(Uuid, u32)> = self.folders
            .iter()
            .filter(|f| f.parent_id == parent_id)
            .map(|f| (f.id, f.order))
            .collect();
        folders.sort_by_key(|(_, order)| *order);
        for (new_order, (id, _)) in folders.into_iter().enumerate() {
            if let Some(folder) = self.folders.iter_mut().find(|f| f.id == id) {
                folder.order = (new_order + 1) as u32;
            }
        }
    }

    pub fn set_folder_expanded(&mut self, folder_id: Uuid, expanded: bool) {
        if let Some(folder) = self.folders.iter_mut().find(|f| f.id == folder_id) {
            folder.expanded = expanded;
        }
    }
}
