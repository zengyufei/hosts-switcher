use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProfileMetadata {
    pub id: String,
    pub name: String,
    pub active: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct AppConfig {
    pub multi_select: bool,
    pub profiles: Vec<ProfileMetadata>,
    pub active_profile_ids: Vec<String>, // Deprecated in favor of internal active flag? Or keep synced? 
                                         // Let's keep synced or just use 'active' field in ProfileMetadata for simplicity.
                                         // Actually, sticking to what I planned: ProfileMetadata has 'active'. 
                                         // But for multi-select logic, we need to know who is active quickly. 
                                         // Let's trust ProfileMetadata.active as source of truth.
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProfileData {
    pub id: String,
    pub name: String,
    pub content: String,
    pub active: bool,
}

fn get_app_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path().app_data_dir().map_err(|e| e.to_string())
}

fn get_profiles_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = get_app_dir(app)?.join("profiles");
    if !dir.exists() {
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    }
    Ok(dir)
}

fn get_config_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(get_app_dir(app)?.join("config.json"))
}

fn get_common_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(get_app_dir(app)?.join("common.txt"))
}

#[tauri::command]
pub fn load_config(app: AppHandle) -> Result<AppConfig, String> {
    let path = get_config_path(&app)?;
    if !path.exists() {
        // First Run: Create defaults
        let mut config = AppConfig::default();
        config.multi_select = false;
        
        let defaults = vec!["Dev", "Test", "Prod"];
        
        // 1. Auto-backup System Hosts
        let sys_id = Uuid::new_v4().to_string();
        let sys_hosts_content = crate::hosts::get_system_hosts(); // Might fail if no permission read? usually read is ok.
        // If fail, empty.
        let sys_content = sys_hosts_content.unwrap_or_else(|_| "# Backup failed".to_string());
        
        save_profile_file(&app, &sys_id, &sys_content)?;
        config.profiles.push(ProfileMetadata {
            id: sys_id,
            name: "系统hosts备份".to_string(),
            active: false,
        });

        // 2. Default Envs
        for name in defaults {
             let id = Uuid::new_v4().to_string();
             save_profile_file(&app, &id, "# New Environment\n")?;
             config.profiles.push(ProfileMetadata {
                 id,
                 name: name.to_string(),
                 active: false,
             });
        }
        
        save_config_internal(&app, &config)?;
        return Ok(config);
    }
    
    let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
    serde_json::from_str(&content).map_err(|e| e.to_string())
}

fn save_config_internal(app: &AppHandle, config: &AppConfig) -> Result<(), String> {
    let path = get_config_path(app)?;
    if let Some(parent) = path.parent() {
        if !parent.exists() {
             fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
    }
    let content = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    fs::write(path, content).map_err(|e| e.to_string())
}

fn save_profile_file(app: &AppHandle, id: &str, content: &str) -> Result<(), String> {
    let dir = get_profiles_dir(app)?;
    let path = dir.join(format!("{}.txt", id));
    fs::write(path, content).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn load_common_config(app: AppHandle) -> Result<String, String> {
    let path = get_common_path(&app)?;
    if !path.exists() {
        return Ok(String::new());
    }
    fs::read_to_string(path).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_common_config(app: AppHandle, content: String) -> Result<(), String> {
    let path = get_common_path(&app)?;
    fs::write(path, content).map_err(|e| e.to_string())?;
    apply_config(app)
}

#[tauri::command]
pub fn list_profiles(app: AppHandle) -> Result<Vec<ProfileData>, String> {
    let config = load_config(app.clone())?;
    let dir = get_profiles_dir(&app)?;
    
    let mut profiles = Vec::new();
    
    for meta in config.profiles {
        let path = dir.join(format!("{}.txt", meta.id));
        let content = if path.exists() {
             fs::read_to_string(&path).unwrap_or_default()
        } else {
             String::new()
        };
        
        profiles.push(ProfileData {
            id: meta.id,
            name: meta.name,
            content,
            active: meta.active,
        });
    }
    
    Ok(profiles)
}

#[tauri::command]
pub fn create_profile(app: AppHandle, name: String, content: Option<String>) -> Result<String, String> {
    let mut config = load_config(app.clone())?;
    
    // Check for duplicate name
    if config.profiles.iter().any(|p| p.name == name) {
        return Err("环境名称已存在 / Profile name already exists".to_string());
    }

    let id = Uuid::new_v4().to_string();
    let initial_content = content.unwrap_or_default();
    save_profile_file(&app, &id, &initial_content)?;
    
    config.profiles.push(ProfileMetadata {
        id: id.clone(),
        name,
        active: false,
    });
    
    save_config_internal(&app, &config)?;
    Ok(id)
}

#[tauri::command]
pub fn save_profile_content(app: AppHandle, id: String, content: String) -> Result<(), String> {
    save_profile_file(&app, &id, &content)?;
    
    // If this profile is active, re-apply config to system hosts
    let config = load_config(app.clone())?;
    if config.profiles.iter().any(|p| p.id == id && p.active) {
        apply_config(app)?;
    }
    Ok(())
}

#[tauri::command]
pub fn delete_profile(app: AppHandle, id: String) -> Result<(), String> {
    let mut config = load_config(app.clone())?;
    
    // Remove from config
    if let Some(idx) = config.profiles.iter().position(|p| p.id == id) {
        config.profiles.remove(idx);
        save_config_internal(&app, &config)?;
    }
    
    // Delete file
    let dir = get_profiles_dir(&app)?;
    let path = dir.join(format!("{}.txt", id));
    if path.exists() {
        let _ = fs::remove_file(path);
    }
    
    Ok(())
}

#[tauri::command]
pub fn rename_profile(app: AppHandle, id: String, new_name: String) -> Result<(), String> {
    let mut config = load_config(app.clone())?;
    
    // Check for duplicate name (excluding itself)
    if config.profiles.iter().any(|p| p.name == new_name && p.id != id) {
        return Err("环境名称已存在 / Profile name already exists".to_string());
    }

    if let Some(idx) = config.profiles.iter().position(|p| p.id == id) {
        config.profiles[idx].name = new_name;
        save_config_internal(&app, &config)?;
    }
    Ok(())
}

#[tauri::command]
pub fn toggle_profile_active(app: AppHandle, id: String) -> Result<(), String> {
     let mut config = load_config(app.clone())?;
     
     if config.multi_select {
         // Toggle specific
         if let Some(p) = config.profiles.iter_mut().find(|p| p.id == id) {
             p.active = !p.active;
         }
     } else {
         // Single select logic
         // If clicking active, toggle off? Or do nothing? Usually toggle off or keep on.
         // Let's say toggle off if already on.
         let was_active = config.profiles.iter().find(|p| p.id == id).map(|p| p.active).unwrap_or(false);
         
         // Turn all off
         for p in &mut config.profiles {
             p.active = false;
         }
         
         // If it wasn't active, turn it on
         if !was_active {
             if let Some(p) = config.profiles.iter_mut().find(|p| p.id == id) {
                 p.active = true;
             }
         }
     }
     
     save_config_internal(&app, &config)?;
     apply_config(app)
}

#[tauri::command]
pub fn set_multi_select(app: AppHandle, enable: bool) -> Result<(), String> {
    let mut config = load_config(app.clone())?;
    config.multi_select = enable;
    
    // If disabling multi-select, and multiple are active, keep only first?
    if !enable {
        let mut found = false;
        for p in &mut config.profiles {
            if p.active {
                if found {
                    p.active = false; 
                } else {
                    found = true;
                }
            }
        }
    }
    
    save_config_internal(&app, &config)?;
    apply_config(app)
}

#[tauri::command]
pub fn apply_config(app: AppHandle) -> Result<(), String> {
    let config = load_config(app.clone())?;
    let common_config = load_common_config(app.clone()).unwrap_or_default();
    
    let profiles_dir = get_profiles_dir(&app)?;
    let mut merged_content = String::from("# Generated by Hostly\n\n");
    merged_content.push_str("### Common Config ###\n");
    merged_content.push_str(&common_config);
    merged_content.push_str("\n\n");

    let read_profile = |id: &str| -> String {
        let path = profiles_dir.join(format!("{}.txt", id));
        if path.exists() {
             fs::read_to_string(path).unwrap_or_default()
        } else {
             String::new()
        }
    };

    for profile in config.profiles {
        if profile.active {
            merged_content.push_str(&format!("### Profile: {} ###\n", profile.name));
            merged_content.push_str(&read_profile(&profile.id));
            merged_content.push_str("\n\n");
        }
    }

    crate::hosts::save_system_hosts(merged_content)
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FullBackup {
    version: i32,
    timestamp: String,
    config: AppConfig,
    common_content: String,
    profiles_content: std::collections::HashMap<String, String>, // id -> content
}

#[tauri::command]
pub fn import_data(app: AppHandle, json_content: String) -> Result<(), String> {
    let backup: FullBackup = serde_json::from_str(&json_content).map_err(|e| format!("Invalid JSON: {}", e))?;
    
    // Restore Common
    save_common_config(app.clone(), backup.common_content)?;
    
    // Restore Config (Metadata)
    save_config_internal(&app, &backup.config)?;
    
    // Restore Profile Files
    for (id, content) in backup.profiles_content {
        save_profile_file(&app, &id, &content)?;
    }
    
    apply_config(app)
}

#[tauri::command]
pub fn export_data(app: AppHandle) -> Result<String, String> {
    let config = load_config(app.clone())?;
    let common_content = load_common_config(app.clone())?;
    
    let dir = get_profiles_dir(&app)?;
    let mut profiles_content = std::collections::HashMap::new();
    
    for p in &config.profiles {
        let path = dir.join(format!("{}.txt", p.id));
        let content = if path.exists() {
             fs::read_to_string(path).unwrap_or_default()
        } else {
             String::new()
        };
        profiles_content.insert(p.id.clone(), content);
    }
    
    let backup = FullBackup {
        version: 2,
        timestamp: chrono::Local::now().to_rfc3339(),
        config,
        common_content,
        profiles_content,
    };
    
    serde_json::to_string_pretty(&backup).map_err(|e| e.to_string())
}

// Helpers for simple file io not needed as much now, but kept for single export if needed
#[tauri::command]
pub fn import_file(path: String) -> Result<String, String> {
    fs::read_to_string(path).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn export_file(path: String, content: String) -> Result<(), String> {
    fs::write(path, content).map_err(|e| e.to_string())
}

// ================= CLI Helpers =================
// These functions are pub but not commands, used by cli.rs

pub fn find_profile_id_by_name(app: &AppHandle, name: &str) -> Result<Option<String>, String> {
    let config = load_config(app.clone())?;
    Ok(config.profiles.into_iter().find(|p| p.name == name).map(|p| p.id))
}

pub fn upsert_profile(app: &AppHandle, name: String, content: String) -> Result<String, String> {
    // Check if exists
    let existing_id = find_profile_id_by_name(app, &name)?;
    
    if let Some(id) = existing_id {
        // Update content
        save_profile_file(app, &id, &content)?;
        Ok(id)
    } else {
        // Create new
        create_profile(app.clone(), name, Some(content))
    }
}

#[tauri::command]
pub fn import_switchhosts(app: AppHandle, json_content: String) -> Result<usize, String> {
    let raw: serde_json::Value = serde_json::from_str(&json_content).map_err(|e| format!("Invalid JSON: {}", e))?;
    
    // SwitchHosts v4+ format: data.list.tree (structure) + data.collection.hosts.data (content)
    if let Some(data) = raw.get("data") {
        let mut content_map = std::collections::HashMap::new();
        
        // Build ID -> Content map
        if let Some(hosts_data) = data.get("collection")
            .and_then(|c| c.get("hosts"))
            .and_then(|h| h.get("data"))
            .and_then(|d| d.as_array()) 
        {
            for h in hosts_data {
                if let (Some(id), Some(content)) = (h.get("id").and_then(|v| v.as_str()), h.get("content").and_then(|v| v.as_str())) {
                    content_map.insert(id, content);
                }
            }
        }

        // Traverse tree
        if let Some(tree) = data.get("list").and_then(|l| l.get("tree")).and_then(|t| t.as_array()) {
            let mut count = 0;
            parse_switchhosts_v4_tree(&app, tree, &content_map, &mut count)?;
            apply_config(app)?;
            return Ok(count);
        }
    }

    // Fallback to simpler format (v1-v3 or simpler exports)
    let list = if let Some(l) = raw.get("list") {
        l.as_array().ok_or("Invalid SwitchHosts format: 'list' is not an array")?
    } else if raw.is_array() {
        raw.as_array().unwrap()
    } else {
        return Err("Invalid SwitchHosts format: Expected SH v4 structure or a simple array".to_string());
    };

    let mut count = 0;
    parse_switchhosts_items(&app, list, &mut count)?;

    apply_config(app)?;
    Ok(count)
}

fn parse_switchhosts_v4_tree(
    app: &AppHandle, 
    items: &Vec<serde_json::Value>, 
    content_map: &std::collections::HashMap<&str, &str>, 
    count: &mut usize
) -> Result<(), String> {
    for item in items {
        let title = item.get("title").and_then(|v| v.as_str()).unwrap_or("Unknown");
        let item_type = item.get("type").and_then(|v| v.as_str()).unwrap_or("local");
        let id = item.get("id").and_then(|v| v.as_str()).unwrap_or("");

        if item_type == "folder" {
            if let Some(children) = item.get("children").and_then(|c| c.as_array()) {
                parse_switchhosts_v4_tree(app, children, content_map, count)?;
            }
        } else {
            // Find content in map or item itself
            let content = content_map.get(id).map(|c| *c).or_else(|| item.get("content").and_then(|v| v.as_str())).unwrap_or("");
            upsert_profile(app, title.to_string(), content.to_string())?;
            *count += 1;
        }
    }
    Ok(())
}

fn parse_switchhosts_items(app: &AppHandle, items: &Vec<serde_json::Value>, count: &mut usize) -> Result<(), String> {
    for item in items {
        let title = item.get("title").and_then(|v| v.as_str()).unwrap_or("Unknown");
        let folder = item.get("folder").and_then(|v| v.as_bool())
            .or_else(|| item.get("type").and_then(|v| Some(v.as_str() == Some("folder"))))
            .unwrap_or(false);
        
        if folder {
            if let Some(children) = item.get("children").and_then(|c| c.as_array()) {
                parse_switchhosts_items(app, children, count)?;
            }
        } else {
            let content = item.get("content").and_then(|v| v.as_str()).unwrap_or("");
            upsert_profile(app, title.to_string(), content.to_string())?;
            *count += 1;
        }
    }
    Ok(())
}

