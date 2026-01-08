use std::fs;
use std::path::PathBuf;

#[cfg(target_os = "windows")]
fn get_hosts_path() -> PathBuf {
    PathBuf::from("C:\\Windows\\System32\\drivers\\etc\\hosts")
}

#[cfg(not(target_os = "windows"))]
fn get_hosts_path() -> PathBuf {
    PathBuf::from("/etc/hosts")
}

#[tauri::command]
pub fn get_system_hosts() -> Result<String, String> {
    let path = get_hosts_path();
    fs::read_to_string(&path).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_system_hosts(content: String) -> Result<(), String> {
    let path = get_hosts_path();
    // Start with a backup? Maybe later. For now, KISS.
    fs::write(&path, content).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn check_write_permission() -> Result<bool, String> {
    let path = get_hosts_path();
    // Try to open the file in append mode. This checks if we have write permissions 
    // without actually modifying or truncating the file.
    let result = std::fs::OpenOptions::new()
        .write(true)
        .append(true)
        .open(&path);
        
    Ok(result.is_ok())
}

#[tauri::command]
pub fn hostly_open_url(url: String) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(&["/C", "start", &url])
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&url)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&url)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
