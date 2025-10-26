use crate::github;
use std::collections::HashSet;
use std::sync::mpsc::Sender;
use std::thread;
use std::time::{Duration, Instant};

const POLLING_INTERVAL_SECONDS: u64 = 60;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MonitoredBranch {
    pub name: String,
    pub last_status_image: String,
}

#[derive(Debug, Default)]
pub struct MonitorState {
    pub branches: HashSet<MonitoredBranch>,
}

pub fn run(gui_sender: Sender<String>, owner: String, repo: String) {
    println!("Monitor: Starting background thread for {}/{}", owner, repo);
    thread::spawn(move || {
        let mut state = MonitorState::default();
        let mut last_poll_time = Instant::now()
            .checked_sub(Duration::from_secs(POLLING_INTERVAL_SECONDS))
            .unwrap();

        loop {
            if last_poll_time.elapsed() >= Duration::from_secs(POLLING_INTERVAL_SECONDS) {
                println!("Monitor: Polling GitHub for updates...");
                let rt = tokio::runtime::Runtime::new().unwrap();

                rt.block_on(async {
                    let remote_branch_names: HashSet<String> =
                        match github::get_all_branch_names(&owner, &repo).await {
                            Ok(names) => names.into_iter().collect(),
                            Err(e) => {
                                eprintln!("Monitor: Error fetching branches: {}", e);
                                return;
                            }
                        };

                    let current_branch_names: HashSet<String> =
                        state.branches.iter().map(|b| b.name.clone()).collect();

                    for name in remote_branch_names.difference(&current_branch_names) {
                        println!("Monitor: Detected new branch \"{}\"", name);
                        state.branches.insert(MonitoredBranch {
                            name: name.clone(),
                            last_status_image: "".to_string(),
                        });
                    }

                    state
                        .branches
                        .retain(|b| remote_branch_names.contains(&b.name));

                    let mut updated_branches = HashSet::new();
                    for mut branch in state.branches.drain() {
                        let new_status_image =
                            github::get_branch_status_image(&owner, &repo, &branch.name).await;

                        if new_status_image != branch.last_status_image
                            && !new_status_image.is_empty()
                        {
                            println!(
                                "Monitor: Status change for branch \"{}\": {}",
                                branch.name, new_status_image
                            );
                            branch.last_status_image = new_status_image.clone();
                            gui_sender.send(new_status_image).unwrap();
                        }
                        updated_branches.insert(branch);
                    }
                    state.branches = updated_branches;
                });

                last_poll_time = Instant::now();
            }

            thread::sleep(Duration::from_secs(1));
        }
    });
}
