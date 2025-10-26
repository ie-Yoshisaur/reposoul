// src/main.rs
use reposouls::{gui, monitor};
use std::error::Error;
use std::fs;
use std::path::Path;
use std::sync::mpsc;

fn main() -> Result<(), Box<dyn Error>> {
    println!("Reposouls: Starting...");

    // Get repository info from the current directory's .git/config
    let (owner, repo) = match get_repo_details_from_current_dir() {
        Ok(details) => {
            println!("Reposouls: Found repository \"{}/\"{}\"", details.0, details.1);
            details
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            eprintln!("Please run this application from the root of a Git repository.");
            return Err(e.into());
        }
    };

    // Graceful shutdown handler for Ctrl-C
    ctrlc::set_handler(move || {
        println!("\nReposouls: Shutdown signal received. Exiting.");
        std::process::exit(0);
    })
    .expect("Error setting Ctrl-C handler");

    // Create a channel for communication between monitor and GUI
    let (gui_sender, gui_receiver) = mpsc::channel::<String>();

    // Start the monitor thread
    monitor::run(gui_sender, owner, repo.clone());

    // Run the GUI on the main thread
    println!("Reposouls: GUI is running. Monitoring \"{}\" in the background.", repo);
    gui::run_gui(gui_receiver)?;

    Ok(())
}

/// Reads the .git/config file in the current directory and extracts the
/// owner and repository name from the remote "origin" URL.
fn get_repo_details_from_current_dir() -> Result<(String, String), String> {
    let config_path = Path::new(".git/config");
    if !config_path.exists() {
        return Err("\".git/config\" not found in the current directory.".to_string());
    }

    let config_content =
        fs::read_to_string(config_path).map_err(|e| format!("Failed to read .git/config: {}", e))?;

    let mut in_remote_origin = false;
    let mut url = None;

    for line in config_content.lines() {
        let trimmed_line = line.trim();
        if trimmed_line == "[remote \"origin\"]" {
            in_remote_origin = true;
        } else if in_remote_origin && trimmed_line.starts_with('[') {
            // Exited the remote origin section
            break;
        } else if in_remote_origin && trimmed_line.starts_with("url =") {
            url = Some(trimmed_line.split('=').nth(1).unwrap_or("").trim().to_string());
            break; // Found the URL, no need to continue
        }
    }

    let remote_url = url.ok_or_else(|| "Could not find remote 'origin' URL in .git/config".to_string())?;

    // Parse owner and repo name from the URL
    // Handles both SSH (git@github.com:owner/repo.git) and HTTPS (https://github.com/owner/repo.git) formats
    let parts: Vec<&str> = remote_url.split(|c| c == '/' || c == ':').collect();
    if parts.len() < 2 {
        return Err(format!("Could not parse owner and repo from URL: {}", remote_url));
    }

    let repo_name_with_git = parts.last().unwrap();
    let repo_name = repo_name_with_git.strip_suffix(".git").unwrap_or(repo_name_with_git);
    let owner = parts.get(parts.len() - 2).unwrap();

    Ok((owner.to_string(), repo_name.to_string()))
}
