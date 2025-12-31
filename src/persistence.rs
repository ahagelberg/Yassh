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
        Ok(())
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

    pub fn add_folder(&mut self, folder: SessionFolder) {
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
        self.folders
            .iter()
            .filter(|f| f.parent_id == parent_id)
            .collect()
    }

    pub fn move_session_to_folder(&mut self, session_id: Uuid, folder_id: Option<Uuid>) {
        // Calculate max order first
        let max_order = self.sessions
            .iter()
            .filter(|s| s.folder_id == folder_id && s.id != session_id)
            .map(|s| s.order)
            .max()
            .unwrap_or(0);
        // Then update the session
        if let Some(session) = self.sessions.iter_mut().find(|s| s.id == session_id) {
            session.folder_id = folder_id;
            session.order = max_order + 1;
        }
    }

    pub fn move_session_relative(&mut self, session_id: Uuid, target_id: Uuid, before: bool) {
        // Get the target session's folder and order
        let target_info = self.sessions
            .iter()
            .find(|s| s.id == target_id)
            .map(|s| (s.folder_id, s.order));
        let Some((target_folder, target_order)) = target_info else { return };
        // Calculate the new order for the moved session
        let new_order = if before {
            target_order
        } else {
            target_order + 1
        };
        // Shift all sessions at or after the new position in the target folder
        for session in self.sessions.iter_mut() {
            if session.folder_id == target_folder && session.id != session_id && session.order >= new_order {
                session.order += 1;
            }
        }
        // Move the session to the target folder and set its order
        if let Some(session) = self.sessions.iter_mut().find(|s| s.id == session_id) {
            session.folder_id = target_folder;
            session.order = new_order;
        }
        // Normalize orders in the target folder
        self.normalize_orders(target_folder);
    }

    pub fn reorder_session(&mut self, session_id: Uuid, target_id: Uuid, before: bool) {
        // Get the folder and target order
        let target_info = self.sessions
            .iter()
            .find(|s| s.id == target_id)
            .map(|s| (s.folder_id, s.order));
        let Some((folder_id, target_order)) = target_info else { return };
        // Calculate the new order for the moved session
        let new_order = if before {
            target_order
        } else {
            target_order + 1
        };
        // Shift all sessions at or after the new position
        for session in self.sessions.iter_mut() {
            if session.folder_id == folder_id && session.id != session_id && session.order >= new_order {
                session.order += 1;
            }
        }
        // Set the moved session's order
        if let Some(session) = self.sessions.iter_mut().find(|s| s.id == session_id) {
            session.order = new_order;
        }
        // Normalize orders to avoid gaps
        self.normalize_orders(folder_id);
    }

    fn normalize_orders(&mut self, folder_id: Option<Uuid>) {
        let mut folder_sessions: Vec<_> = self.sessions
            .iter()
            .enumerate()
            .filter(|(_, s)| s.folder_id == folder_id)
            .map(|(i, s)| (i, s.order))
            .collect();
        folder_sessions.sort_by_key(|(_, order)| *order);
        for (new_order, (idx, _)) in folder_sessions.into_iter().enumerate() {
            self.sessions[idx].order = new_order as u32;
        }
    }

    pub fn move_folder_to_parent(&mut self, folder_id: Uuid, parent_id: Option<Uuid>) {
        if let Some(folder) = self.folders.iter_mut().find(|f| f.id == folder_id) {
            folder.parent_id = parent_id;
        }
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
}
