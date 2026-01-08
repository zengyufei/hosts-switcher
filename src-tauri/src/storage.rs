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

pub enum Context<'a> {
    Tauri(&'a AppHandle),
    Headless,
}

impl<'a> Context<'a> {
    pub fn get_app_dir(&self) -> Result<PathBuf, String> {
        match self {
            Context::Tauri(app) => app.path().app_data_dir().map_err(|e| e.to_string()),
            Context::Headless => {
                // Hardcoded fallback for headless CLI to match Tauri's app_data_dir for "com.hostly.app"
                #[cfg(target_os = "windows")]
                {
                    let base = std::env::var("APPDATA").map(PathBuf::from).map_err(|_| "APPDATA env var not found")?;
                    Ok(base.join("com.hostly.switcher"))
                }
                #[cfg(target_os = "macos")]
                {
                    let home = std::env::var("HOME").map(PathBuf::from).map_err(|_| "HOME env var not found")?;
                    Ok(home.join("Library/Application Support/com.hostly.switcher"))
                }
                #[cfg(target_os = "linux")]
                {
                    if let Ok(data_home) = std::env::var("XDG_DATA_HOME") {
                        Ok(PathBuf::from(data_home).join("com.hostly.switcher"))
                    } else {
                        let home = std::env::var("HOME").map(PathBuf::from).map_err(|_| "HOME env var not found")?;
                        Ok(home.join(".local/share/com.hostly.switcher"))
                    }
                }
            }
        }
    }
}

fn get_profiles_dir(ctx: &Context) -> Result<PathBuf, String> {
    let dir = ctx.get_app_dir()?.join("profiles");
    if !dir.exists() {
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    }
    Ok(dir)
}

fn get_config_path(ctx: &Context) -> Result<PathBuf, String> {
    Ok(ctx.get_app_dir()?.join("config.json"))
}

fn get_common_path(ctx: &Context) -> Result<PathBuf, String> {
    Ok(ctx.get_app_dir()?.join("common.txt"))
}

#[tauri::command]
pub fn load_config(app: AppHandle) -> Result<AppConfig, String> {
    load_config_internal(&Context::Tauri(&app))
}

pub fn load_config_internal(ctx: &Context) -> Result<AppConfig, String> {
    let path = get_config_path(ctx)?;
    if !path.exists() {
        // First Run: Create defaults
        let mut config = AppConfig::default();
        config.multi_select = false;
        
        let defaults = vec!["Dev", "Test", "Prod"];
        
        // 1. Auto-backup System Hosts
        let sys_id = Uuid::new_v4().to_string();
        let sys_hosts_content = crate::hosts::get_system_hosts();
        let sys_content = sys_hosts_content.unwrap_or_else(|_| "# Backup failed".to_string());
        
        save_profile_file_internal(ctx, &sys_id, &sys_content)?;
        config.profiles.push(ProfileMetadata {
            id: sys_id,
            name: "系统hosts备份".to_string(),
            active: false,
        });

        // 2. Default Envs
        for name in defaults {
             let id = Uuid::new_v4().to_string();
             save_profile_file_internal(ctx, &id, "# New Environment\n")?;
             config.profiles.push(ProfileMetadata {
                 id,
                 name: name.to_string(),
                 active: false,
             });
        }
        
        save_config_internal(ctx, &config)?;
        return Ok(config);
    }
    
    let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
    serde_json::from_str(&content).map_err(|e| e.to_string())
}

pub fn save_config_internal(ctx: &Context, config: &AppConfig) -> Result<(), String> {
    let path = get_config_path(ctx)?;
    if let Some(parent) = path.parent() {
        if !parent.exists() {
             fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
    }
    let content = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    fs::write(path, content).map_err(|e| e.to_string())
}

pub fn save_profile_file_internal(ctx: &Context, id: &str, content: &str) -> Result<(), String> {
    let dir = get_profiles_dir(ctx)?;
    let path = dir.join(format!("{}.txt", id));
    fs::write(path, content).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn load_common_config(app: AppHandle) -> Result<String, String> {
    load_common_config_internal(&Context::Tauri(&app))
}

pub fn load_common_config_internal(ctx: &Context) -> Result<String, String> {
    let path = get_common_path(ctx)?;
    if !path.exists() {
        return Ok(String::new());
    }
    fs::read_to_string(path).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_common_config(app: AppHandle, content: String) -> Result<(), String> {
    save_common_config_internal(&Context::Tauri(&app), content)?;
    apply_config(app)
}

pub fn save_common_config_internal(ctx: &Context, content: String) -> Result<(), String> {
    let path = get_common_path(ctx)?;
    fs::write(path, content).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_profiles(app: AppHandle) -> Result<Vec<ProfileData>, String> {
    list_profiles_internal(&Context::Tauri(&app))
}

pub fn list_profiles_internal(ctx: &Context) -> Result<Vec<ProfileData>, String> {
    let config = load_config_internal(ctx)?;
    let dir = get_profiles_dir(ctx)?;
    
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
    create_profile_internal(&Context::Tauri(&app), name, content)
}

pub fn create_profile_internal(ctx: &Context, name: String, content: Option<String>) -> Result<String, String> {
    let mut config = load_config_internal(ctx)?;
    
    // Check for duplicate name
    if config.profiles.iter().any(|p| p.name == name) {
        return Err("环境名称已存在 / Profile name already exists".to_string());
    }

    let id = Uuid::new_v4().to_string();
    let initial_content = content.unwrap_or_default();
    save_profile_file_internal(ctx, &id, &initial_content)?;
    
    config.profiles.push(ProfileMetadata {
        id: id.clone(),
        name,
        active: false,
    });
    
    save_config_internal(ctx, &config)?;
    Ok(id)
}

#[tauri::command]
pub fn save_profile_content(app: AppHandle, id: String, content: String) -> Result<(), String> {
    let ctx = Context::Tauri(&app);
    save_profile_content_internal(&ctx, &id, &content)?;
    
    // If this profile is active, re-apply config to system hosts
    let config = load_config_internal(&ctx)?;
    if config.profiles.iter().any(|p| p.id == id && p.active) {
        apply_config(app)?;
    }
    Ok(())
}

pub fn save_profile_content_internal(ctx: &Context, id: &str, content: &str) -> Result<(), String> {
    save_profile_file_internal(ctx, id, content)
}

#[tauri::command]
pub fn delete_profile(app: AppHandle, id: String) -> Result<(), String> {
    delete_profile_internal(&Context::Tauri(&app), &id)
}

pub fn delete_profile_internal(ctx: &Context, id: &str) -> Result<(), String> {
    let mut config = load_config_internal(ctx)?;
    
    // Remove from config
    if let Some(idx) = config.profiles.iter().position(|p| p.id == id) {
        config.profiles.remove(idx);
        save_config_internal(ctx, &config)?;
    }
    
    // Delete file
    let dir = get_profiles_dir(ctx)?;
    let path = dir.join(format!("{}.txt", id));
    if path.exists() {
        let _ = fs::remove_file(path);
    }
    
    Ok(())
}

#[tauri::command]
pub fn rename_profile(app: AppHandle, id: String, new_name: String) -> Result<(), String> {
    rename_profile_internal(&Context::Tauri(&app), &id, new_name)
}

pub fn rename_profile_internal(ctx: &Context, id: &str, new_name: String) -> Result<(), String> {
    let mut config = load_config_internal(ctx)?;
    
    // Check for duplicate name (excluding itself)
    if config.profiles.iter().any(|p| p.name == new_name && p.id != id) {
        return Err("环境名称已存在 / Profile name already exists".to_string());
    }

    if let Some(idx) = config.profiles.iter().position(|p| p.id == id) {
        config.profiles[idx].name = new_name;
        save_config_internal(ctx, &config)?;
    }
    Ok(())
}

#[tauri::command]
pub fn toggle_profile_active(app: AppHandle, id: String) -> Result<(), String> {
    toggle_profile_active_internal(&Context::Tauri(&app), &id)?;
    apply_config(app)
}

pub fn toggle_profile_active_internal(ctx: &Context, id: &str) -> Result<(), String> {
    let mut config = load_config_internal(ctx)?;
    
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
    
    save_config_internal(ctx, &config)
}

#[tauri::command]
pub fn set_multi_select(app: AppHandle, enable: bool) -> Result<(), String> {
    set_multi_select_internal(&Context::Tauri(&app), enable)?;
    apply_config(app)
}

pub fn set_multi_select_internal(ctx: &Context, enable: bool) -> Result<(), String> {
    let mut config = load_config_internal(ctx)?;
    config.multi_select = enable;
    
    // If disabling multi-select, and multiple are active, keep only first
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
    
    save_config_internal(ctx, &config)
}

#[tauri::command]
pub fn apply_config(app: AppHandle) -> Result<(), String> {
    apply_config_internal(&Context::Tauri(&app))
}

pub fn apply_config_internal(ctx: &Context) -> Result<(), String> {
    let config = load_config_internal(ctx)?;
    let common_config = load_common_config_internal(ctx).unwrap_or_default();
    
    let profiles_dir = get_profiles_dir(ctx)?;
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
    // Support both new (Vec) and old (HashMap) formats for compatibility
    profiles: Option<Vec<ProfileData>>,
    profiles_content: Option<std::collections::HashMap<String, String>>,
}

#[tauri::command]
pub fn import_data(app: AppHandle, json_content: String) -> Result<(), String> {
    import_data_internal(&Context::Tauri(&app), json_content)?;
    apply_config(app)
}

pub fn import_data_internal(ctx: &Context, json_content: String) -> Result<(), String> {
    let backup: FullBackup = serde_json::from_str(&json_content).map_err(|e| e.to_string())?;
    
    // Reset config
    save_config_internal(ctx, &backup.config)?;
    
    // Save each profile (New Version: Vec<ProfileData>)
    if let Some(profiles) = backup.profiles {
        for profile in profiles {
            save_profile_file_internal(ctx, &profile.id, &profile.content)?;
        }
    } 
    // Save each profile (Old Version: HashMap<id, content>)
    else if let Some(profiles_content) = backup.profiles_content {
        for (id, content) in profiles_content {
            save_profile_file_internal(ctx, &id, &content)?;
        }
    }
    
    Ok(())
}

#[tauri::command]
pub fn export_data(app: AppHandle) -> Result<String, String> {
    export_data_internal(&Context::Tauri(&app))
}

pub fn export_data_internal(ctx: &Context) -> Result<String, String> {
    let config = load_config_internal(ctx)?;
    let profiles = list_profiles_internal(ctx)?;
    
    let backup = FullBackup {
        version: 2,
        timestamp: chrono::Local::now().to_rfc3339(),
        config,
        profiles: Some(profiles),
        profiles_content: None,
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
#[tauri::command]
pub fn find_profile_id_by_name(app: AppHandle, name: String) -> Result<Option<String>, String> {
    find_profile_id_by_name_internal(&Context::Tauri(&app), &name)
}

pub fn find_profile_id_by_name_internal(ctx: &Context, name: &str) -> Result<Option<String>, String> {
    let config = load_config_internal(ctx)?;
    Ok(config.profiles.iter().find(|p| p.name == name).map(|p| p.id.clone()))
}

#[tauri::command]
pub fn upsert_profile(app: AppHandle, name: String, content: String) -> Result<String, String> {
    upsert_profile_internal(&Context::Tauri(&app), name, content)
}

pub fn upsert_profile_internal(ctx: &Context, name: String, content: String) -> Result<String, String> {
    if let Some(id) = find_profile_id_by_name_internal(ctx, &name)? {
        save_profile_file_internal(ctx, &id, &content)?;
        Ok(id)
    } else {
        create_profile_internal(ctx, name, Some(content))
    }
}

#[tauri::command]
pub fn import_switchhosts(app: AppHandle, json_content: String) -> Result<usize, String> {
    let ctx = Context::Tauri(&app);
    let count = import_switchhosts_internal(&ctx, json_content)?;
    apply_config(app)?;
    Ok(count)
}

pub fn import_switchhosts_internal(ctx: &Context, json_content: String) -> Result<usize, String> {
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
            parse_switchhosts_v4_tree_internal(ctx, tree, &content_map, &mut count)?;
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
    parse_switchhosts_items_internal(ctx, list, &mut count)?;

    Ok(count)
}

fn parse_switchhosts_v4_tree_internal(
    ctx: &Context, 
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
                parse_switchhosts_v4_tree_internal(ctx, children, content_map, count)?;
            }
        } else {
            // Find content in map or item itself
            let content = content_map.get(id).map(|c| *c).or_else(|| item.get("content").and_then(|v| v.as_str())).unwrap_or("");
            upsert_profile_internal(ctx, title.to_string(), content.to_string())?;
            *count += 1;
        }
    }
    Ok(())
}

fn parse_switchhosts_items_internal(ctx: &Context, items: &Vec<serde_json::Value>, count: &mut usize) -> Result<(), String> {
    for item in items {
        let title = item.get("title").and_then(|v| v.as_str()).unwrap_or("Unknown");
        let folder = item.get("folder").and_then(|v| v.as_bool())
            .or_else(|| item.get("type").and_then(|v| Some(v.as_str() == Some("folder"))))
            .unwrap_or(false);
        
        if folder {
            if let Some(children) = item.get("children").and_then(|c| c.as_array()) {
                parse_switchhosts_items_internal(ctx, children, count)?;
            }
        } else {
            let content = item.get("content").and_then(|v| v.as_str()).unwrap_or("");
            upsert_profile_internal(ctx, title.to_string(), content.to_string())?;
            *count += 1;
        }
    }
    Ok(())
}

