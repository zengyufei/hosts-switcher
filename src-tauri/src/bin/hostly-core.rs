use hostly_lib::cli;

fn main() {
    // In headless mode, we just run the CLI logic directly without a Tauri app shell.
    // The run_cli function will handle the arguments and exit when done.
    if !cli::run_cli(None) {
        // If run_cli returns false, it means no commands were provided.
        // For the headless version, we might want to print help in this case.
        println!("Hostly-Core: Headless CLI for Hosts Management.");
        println!("Use --help to see available commands.");
    }
}
