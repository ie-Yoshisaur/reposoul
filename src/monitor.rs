// src/monitor.rs

//! The core background service and state management for the application.

use crate::github;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::mpsc::Sender;
use std::thread;
use std::time::{Duration, Instant};

const STATE_FILE_NAME: &str = ".reposouls_state.json";
const POLLING_INTERVAL_SECONDS: u64 = 10;

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct MonitorState {
    // Key: branch name
    pub branches: HashMap<String, MonitoredBranch>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MonitoredBranch {
    pub last_notified_sha: String,
    pub last_notified_status: String,
}

// --- State Management ---

fn load_state() -> Result<MonitorState, String> {
    if !Path::new(STATE_FILE_NAME).exists() {
        return Ok(MonitorState::default());
    }
    let content = fs::read_to_string(STATE_FILE_NAME).map_err(|e| e.to_string())?;
    serde_json::from_str(&content).map_err(|e| e.to_string())
}

fn save_state(state: &MonitorState) -> Result<(), String> {
    let content = serde_json::to_string_pretty(state).map_err(|e| e.to_string())?;
    fs::write(STATE_FILE_NAME, content).map_err(|e| e.to_string())
}

// --- Main Service Logic ---

pub fn run(gui_sender: Sender<String>, owner: String, repo: String) {
    println!("Monitor: Starting background thread for {}/{}", owner, repo);
    thread::spawn(move || {
        let mut state = load_state().unwrap_or_else(|e| {
            eprintln!("Monitor: Failed to load state, starting fresh: {}", e);
            MonitorState::default()
        });

        let mut last_poll_time = Instant::now()
            .checked_sub(Duration::from_secs(POLLING_INTERVAL_SECONDS))
            .unwrap();

        loop {
            if last_poll_time.elapsed() >= Duration::from_secs(POLLING_INTERVAL_SECONDS) {
                println!("Monitor: Polling GitHub for updates...");
                let rt = tokio::runtime::Runtime::new().unwrap();
                let client = match github::build_client() {
                    Ok(c) => c,
                    Err(e) => {
                        eprintln!("Monitor: Failed to build GitHub client: {}", e);
                        thread::sleep(Duration::from_secs(POLLING_INTERVAL_SECONDS));
                        continue;
                    }
                };

                let mut state_changed = false;

                rt.block_on(async {
                    // Step 1: Sync branch list
                    let remote_branch_names: Vec<String> = match github::get_all_branch_names(&owner, &repo).await {
                        Ok(names) => names,
                        Err(e) => {
                            eprintln!("Monitor: Error fetching branches: {}", e);
                            return;
                        }
                    };
                    state.branches.retain(|k, _| remote_branch_names.contains(k));

                    // Step 2: Check status for all branches
                    for branch_name in &remote_branch_names {
                        let latest_sha = match github::get_latest_commit_sha(&client, &owner, &repo, branch_name).await {
                            Some(sha) => sha,
                            None => continue, // Branch might have been deleted just now
                        };

                        let new_status = github::get_branch_status_image(&owner, &repo, branch_name).await;

                        let current_branch_state = state.branches.get(branch_name);
                        let is_new_branch = current_branch_state.is_none();
                        let has_new_status = match current_branch_state {
                            Some(b) => b.last_notified_sha != latest_sha || b.last_notified_status != new_status,
                            None => true, // It's a new branch, so we treat it as having a new status to record it.
                        };

                        if has_new_status && !new_status.is_empty() {
                            // Only send a notification if the branch is not new.
                            if !is_new_branch {
                                println!("Monitor: Status change for branch \"{}\": {}", branch_name, new_status);
                                gui_sender.send(new_status.clone()).unwrap();
                            } else {
                                println!("Monitor: New branch \"{}\" detected. Will notify on next status change.", branch_name);
                            }

                            // Always update the state.
                            let new_branch_state = MonitoredBranch {
                                last_notified_sha: latest_sha,
                                last_notified_status: new_status,
                            };
                            state.branches.insert(branch_name.clone(), new_branch_state);
                            state_changed = true;
                        }
                    }
                });

                if state_changed {
                    save_state(&state).unwrap_or_else(|e| eprintln!("Failed to save state: {}", e));
                }
                last_poll_time = Instant::now();
            }

            thread::sleep(Duration::from_secs(1));
        }
    });
}

