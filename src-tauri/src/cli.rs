use clap::{Parser, Subcommand};
use crate::storage;
use tauri::AppHandle;
use std::path::PathBuf;
use std::fs;



#[cfg(windows)]
fn check_elevation() {
    // Simple check: try to open the physical drive? No, too invasive.
    // Try to open SC manager? 
    // Let's use a simple reliable check: `net session`
    let output = std::process::Command::new("net")
        .arg("session")
        .output();
    match output {
        Ok(out) => {
             if out.status.success() {
                 println!("[DIAGNOSTIC] Running as ADMIN");
             } else {
                 println!("[DIAGNOSTIC] Running as STANDARD USER");
             }
        },
        Err(_) => println!("[DIAGNOSTIC] Failed to check elevation"),
    }
}
#[cfg(not(windows))]
fn check_elevation() {}

#[derive(Parser)]
#[command(name = "hostly")]
#[command(version = "1.0")]

struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// List all profiles
    List,
    /// Enable single selection mode
    Single,
    /// Enable multi selection mode
    Multi,
    /// Open/Activate specific profiles
    Open {
        /// Profile names to activate
        #[arg(required = true)]
        names: Vec<String>,

        /// Force multi-select mode if multiple profiles are provided
        #[arg(long, short)]
        multi: bool,
    },
    /// Close/Deactivate specific profiles
    Close {
        /// Profile names to deactivate
        #[arg(required = true)]
        names: Vec<String>,
    },
    /// Export profile(s) or global backup
    Export {
        /// Profile name to export (Optional, exports full backup if missing)
        name: Option<String>,
        
        /// Output file path
        #[arg(long, short, required = true)]
        target: String,
    },
    /// Import profile or common config
    Import {
        /// Profile name to import as. If missing, imports as Common Config.
        name: Option<String>,
        
        /// Input file path
        #[arg(long, short, required = true)]
        target: String,

        /// Activate specific profiles after import. If no profiles listed, activates the imported profile (if named).
        #[arg(long, num_args(0..))]
        open: Option<Vec<String>>,

        /// Force multi-mode if needed (during open)
        #[arg(long, short)]
        multi: bool,
    }
}

pub fn run_cli(app: &AppHandle) -> bool {
    #[cfg(windows)]
    check_elevation();

    // We need to parse args. 
    // clap::Parser::parse() reads from std::env::args().
    // If tauri app is run, first arg is binary path. 
    // If we have no args (length 1), we return false to let GUI run.
    let args: Vec<String> = std::env::args().collect();
    if args.len() <= 1 {
        return false;
    }

    // Try parsing. If it fails (e.g. invalid command), clap usually prints help and exits.
    // However, if we just run `hostly.exe`, we want GUI.
    // We already checked len <= 1. 
    // But what if user runs `hostly.exe --random-flag`? Clap will error.
    // That's fine, we want CLI behavior if args are present.

    let cli = match Cli::try_parse() {
        Ok(c) => c,
        Err(e) => {
            // If error is just help or version, print and exit.
            // If unknown command, print error and exit.
            // But we must distinguish if it's meant for Tauri?
            // Tauri doesn't really take args unless configured.
            e.print().unwrap();
            return true; // Exit app
        }
    };

    match cli.command {
        Some(Commands::List) => {
            match storage::list_profiles(app.clone()) {
                Ok(profiles) => {
                    for p in profiles {
                        println!("{} [{}]", p.name, if p.active { "ACTIVE" } else { "OFF" });
                    }
                }
                Err(e) => eprintln!("Error listing profiles: {}", e),
            }
        },
        Some(Commands::Single) => {
            if let Err(e) = storage::set_multi_select(app.clone(), false) {
                eprintln!("Error setting single mode: {}", e);
            } else {
                 println!("Single selection mode enabled.");
            }
        },
        Some(Commands::Multi) => {
             if let Err(e) = storage::set_multi_select(app.clone(), true) {
                eprintln!("Error setting multi mode: {}", e);
            } else {
                 println!("Multi selection mode enabled.");
            }
        },
        Some(Commands::Open { names, multi }) => {
            if multi {
                if let Err(e) = storage::set_multi_select(app.clone(), true) {
                    eprintln!("Error enabling multi-mode: {}", e);
                    return true;
                }
            }

            // Check mode
            let config = storage::load_config(app.clone()).unwrap_or_default();
            if !config.multi_select && names.len() > 1 {
                eprintln!("Warning: Single select mode is active. Only the first profile '{}' will be activated.", names[0]);
                eprintln!("Use --multi to enable multi-select mode automatically.");
            }

            for name in names {
                if let Ok(Some(id)) = storage::find_profile_id_by_name(app, &name) {
                    // Logic: Toggle if not active
                    // toggle_profile_active command toggles. 
                    // We want "Open" i.e. Ensure Active.
                    // But backend `toggle_profile_active` logic is:
                    // Multi: flip boolean.
                    // Single: if active, turn all off? if inactive, turn it on (and others off).
                    
                    // We need a proper `set_active(id, true)` in backend or reuse toggle carefully.
                    // Let's check state first.
                    let current_profiles = storage::list_profiles(app.clone()).unwrap_or_default();
                    let p = current_profiles.iter().find(|p| p.id == id);
                    if let Some(prof) = p {
                        if !prof.active {
                             if let Err(e) = storage::toggle_profile_active(app.clone(), id) {
                                  eprintln!("Failed to open '{}': {}", name, e);
                             } else {
                                  println!("Opened '{}'", name);
                             }
                        } else {
                             println!("'{}' is already active.", name);
                        }
                    }
                } else {
                     eprintln!("Profile '{}' not found.", name);
                }
            }
        },
        Some(Commands::Close { names }) => {
             for name in names {
                 if let Ok(Some(id)) = storage::find_profile_id_by_name(app, &name) {
                      let current_profiles = storage::list_profiles(app.clone()).unwrap_or_default();
                      if let Some(prof) = current_profiles.iter().find(|p| p.id == id) {
                           if prof.active {
                                // Toggle to turn off
                                if let Err(e) = storage::toggle_profile_active(app.clone(), id) {
                                    eprintln!("Failed to close '{}': {}", name, e);
                                } else {
                                    println!("Closed '{}'", name);
                                }
                           } else {
                                println!("'{}' is already closed.", name);
                           }
                      }
                 } else {
                      eprintln!("Profile '{}' not found.", name);
                 }
             }
        },
        Some(Commands::Export { name, target }) => {
            if let Some(n) = name {
                // Export Single
                if let Ok(Some(id)) = storage::find_profile_id_by_name(app, &n) {
                     let current_profiles = storage::list_profiles(app.clone()).unwrap_or_default();
                     if let Some(p) = current_profiles.iter().find(|p| p.id == id) {
                          if let Err(e) = fs::write(&target, &p.content) {
                               eprintln!("Failed to write file: {}", e);
                          } else {
                               println!("Exported '{}' to '{}'", n, target);
                          }
                     }
                } else {
                     eprintln!("Profile '{}' not found.", n);
                }
            } else {
                // Export All
                match storage::export_data(app.clone()) {
                     Ok(json) => {
                          if let Err(e) = fs::write(&target, json) {
                               eprintln!("Failed to write export file: {}", e);
                          } else {
                               println!("Full backup exported to '{}'", target);
                          }
                     },
                     Err(e) => eprintln!("Export failed: {}", e),
                }
            }
        },
        Some(Commands::Import { name, target, open, multi }) => {
             let path = PathBuf::from(&target);
             if !path.exists() {
                 eprintln!("Target file '{}' not found.", target);
                 return true;
             }

             let content = match fs::read_to_string(&path) {
                 Ok(c) => c,
                 Err(e) => {
                      eprintln!("Failed to read file: {}", e);
                      return true;
                 }
             };

             // Define profiles to open list
             let mut profiles_to_open = Vec::new();
             
             // If --open is present
             if let Some(args) = open {
                 if args.is_empty() {
                     // Empty args: implies opening the IMPORTED profile (if named)
                     if let Some(n) = &name {
                         profiles_to_open.push(n.clone());
                     }
                 } else {
                     // Explicit args
                     profiles_to_open = args;
                 }
             }



             if let Some(n) = name {
                  // Import specific profile
                  match storage::upsert_profile(app, n.clone(), content) {
                       Ok(_) => {
                            println!("Imported profile '{}'.", n);
                       },
                       Err(e) => eprintln!("Import failed: {}", e)
                  }
             } else {
                  // No name specified. Check formatting.
                  // If it ends with .json, assume it's a global backup.
                  if target.to_lowercase().ends_with(".json") {
                      match storage::import_data(app.clone(), content) {
                          Ok(_) => println!("Global backup imported from '{}'.", target),
                          Err(e) => eprintln!("Failed to import global backup: {}", e),
                      }
                  } else {
                       // Otherwise treat as Common Config
                       match storage::save_common_config(app.clone(), content) {
                            Ok(_) => {
                                 println!("Common config updated from '{}'.", target);
                                 let _ = storage::apply_config(app.clone());
                            },
                            Err(e) => eprintln!("Failed to save common config: {}", e)
                       }
                  }
             }

             // Auto Multi-mode check
             if profiles_to_open.len() > 1 || multi {
                  if let Err(e) = storage::set_multi_select(app.clone(), true) {
                      eprintln!("Error enabling multi-select mode: {}", e);
                  } else {
                      if profiles_to_open.len() > 1 {
                          println!("Auto-enabled multi-select mode for {} profiles.", profiles_to_open.len());
                      }
                  }
             }

             // Post-import Open Logic
             for p_name in profiles_to_open {
                 if let Ok(Some(pid)) = storage::find_profile_id_by_name(app, &p_name) {
                      let list = storage::list_profiles(app.clone()).unwrap_or_default();
                      if let Some(p) = list.iter().find(|p| p.id == pid) {
                           if !p.active {
                                let _ = storage::toggle_profile_active(app.clone(), pid);
                                println!("Profile '{}' activated.", p_name);
                           } else {
                                println!("Profile '{}' is already active.", p_name);
                           }
                      }
                 } else {
                      eprintln!("Warning: Cannot open profile '{}' (not found).", p_name);
                 }
             }
        },
        None => return false // No subcommand, run GUI
    }

    true // Command executed, exit app
}
