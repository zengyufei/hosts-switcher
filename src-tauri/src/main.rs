// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]





fn check_admin_and_relaunch() {
    #[cfg(target_os = "windows")]
    {
         let output = std::process::Command::new("net")
            .arg("session")
            .output();
            
         let is_admin = match output {
             Ok(o) => o.status.success(),
             Err(_) => false,
         };

         if !is_admin {
             println!("Not running as admin, attempting to relaunch with RunAs...");
             let current_exe = std::env::current_exe().unwrap();
             let args: Vec<String> = std::env::args().skip(1).collect();
             let args_str = args.iter().map(|arg| {
                 if arg.contains(' ') { format!("\"{}\"", arg) } else { arg.to_string() }
             }).collect::<Vec<String>>().join(" ");

             let mut cmd = std::process::Command::new("powershell");
             cmd.arg("Start-Process");
             cmd.arg(current_exe);
             if !args_str.is_empty() {
                 cmd.arg("-ArgumentList");
                 cmd.arg(format!("'{}'", args_str));
             }
             cmd.arg("-Verb");
             cmd.arg("RunAs");
             
             if let Ok(s) = cmd.status() {
                 if s.success() {
                     std::process::exit(0);
                 }
             }
         }
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        let is_root = std::process::Command::new("id")
            .arg("-u")
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "0")
            .unwrap_or(false);

        if !is_root {
            #[cfg(target_os = "macos")]
            {
                println!("Not running as root, attempting to relaunch with osascript...");
                let current_exe = std::env::current_exe().unwrap();
                let args: Vec<String> = std::env::args().skip(1).collect();
                let args_str = args.iter().map(|arg| {
                    if arg.contains(' ') { format!("'{}'", arg) } else { arg.to_string() }
                }).collect::<Vec<String>>().join(" ");

                let script = format!(
                    "do shell script \"'{}' {}\" with administrator privileges",
                    current_exe.display(),
                    args_str
                );

                if let Ok(s) = std::process::Command::new("osascript").arg("-e").arg(script).status() {
                    if s.success() {
                        std::process::exit(0);
                    }
                }
            }

            #[cfg(target_os = "linux")]
            {
                println!("Error: Modifying hosts requires root privileges. Please run with sudo.");
            }
        }
    }
}

fn main() {
    println!("Starting Hostly...");
    check_admin_and_relaunch();

    hostly_lib::run()
}
