use crate::git::{GitInfo, get_git_info};
use crate::github::{GitHubClient, ReviewState, WorkflowRunConclusion, WorkflowRunStatus};
use chrono::{DateTime, Utc};
use std::collections::HashSet;
use std::env;
use std::sync::mpsc;
use tokio::time::{self, Duration};

#[derive(Debug, PartialEq, Eq)]
pub enum NotificationEvent {
    CiSuccess,
    CiFailure,
    PrApproved,
    PrChangesRequested,
    PrMerged,
    PrNewComment,
}

struct EventCheckerState {
    start_time: DateTime<Utc>,
    seen_workflow_runs: HashSet<i64>,
    seen_comments: HashSet<i64>,
    seen_reviews: HashSet<i64>,
    pr_is_merged: bool,
}

impl EventCheckerState {
    fn new() -> Self {
        Self {
            start_time: Utc::now(),
            seen_workflow_runs: HashSet::new(),
            seen_comments: HashSet::new(),
            seen_reviews: HashSet::new(),
            pr_is_merged: false,
        }
    }
}

pub async fn run_event_checker(image_sender: mpsc::Sender<NotificationEvent>) {
    let token = env::var("GITHUB_TOKEN").expect("GITHUB_TOKEN environment variable not set");
    let git_info = match get_git_info() {
        Ok(info) => info,
        Err(e) => {
            eprintln!(
                "Fatal: Could not get git info. Are you in a git repository? Error: {}",
                e
            );
            return;
        }
    };

    let client = GitHubClient::new(git_info.owner.clone(), git_info.repo.clone(), token);
    println!(
        "Monitoring repository: {}/{}",
        git_info.owner, git_info.repo
    );
    println!("Branch: {}", git_info.branch);

    let mut state = EventCheckerState::new();
    let mut interval = time::interval(Duration::from_secs(10));

    loop {
        interval.tick().await;
        println!("[{}] Checking for events...", Utc::now().format("%H:%M:%S"));

        check_workflow_run(&client, &git_info, &mut state, &image_sender).await;

        if !state.pr_is_merged {
            check_pr_events(&client, &git_info, &mut state, &image_sender).await;
        }
    }
}

async fn check_workflow_run(
    client: &GitHubClient,
    git_info: &GitInfo,
    state: &mut EventCheckerState,
    sender: &mpsc::Sender<NotificationEvent>,
) {
    match client
        .get_workflow_runs_for_branch(&git_info.branch, state.start_time)
        .await
    {
        Ok(runs) => {
            let new_completed_runs: Vec<_> = runs
                .into_iter()
                .filter(|run| {
                    run.status == WorkflowRunStatus::Completed
                        && !state.seen_workflow_runs.contains(&run.id)
                })
                .collect();

            if new_completed_runs.is_empty() {
                return;
            }

            let mut event_to_send: Option<NotificationEvent> = None;

            if new_completed_runs
                .iter()
                .any(|run| run.conclusion == Some(WorkflowRunConclusion::Failure))
            {
                println!("CI Failure detected in new workflow runs.");
                event_to_send = Some(NotificationEvent::CiFailure);
            } else if new_completed_runs
                .iter()
                .any(|run| run.conclusion == Some(WorkflowRunConclusion::Success))
            {
                println!("CI Success detected in new workflow runs.");
                event_to_send = Some(NotificationEvent::CiSuccess);
            }

            if let Some(event) = event_to_send {
                if sender.send(event).is_err() {
                    eprintln!("Failed to send to GUI thread.");
                }
            }

            for run in new_completed_runs {
                state.seen_workflow_runs.insert(run.id);
            }
        }
        Err(e) => eprintln!("Error fetching workflow runs: {}", e),
    }
}

async fn check_pr_events(
    client: &GitHubClient,
    git_info: &GitInfo,
    state: &mut EventCheckerState,
    sender: &mpsc::Sender<NotificationEvent>,
) {
    let pr = match client.get_pr_for_branch(&git_info.branch).await {
        Ok(Some(pr)) => pr,
        Ok(None) => return, // No open PR for this branch, this is normal
        Err(e) => {
            eprintln!("Error fetching PR for branch: {}", e);
            return;
        }
    };

    // Check for merge events
    match client.get_pr_details(pr.number).await {
        Ok(pr_details) => {
            if pr_details.merged == Some(true)
                && pr_details
                    .merged_at
                    .map_or(false, |ts| ts > state.start_time)
            {
                println!("PR #{} was merged!", pr.number);
                if sender.send(NotificationEvent::PrMerged).is_err() {
                    eprintln!("Failed to send to GUI thread. Exiting check_pr_events.");
                    return;
                }
                state.pr_is_merged = true;
                return; // PR is merged, no need to check for other PR events
            }
        }
        Err(e) => eprintln!("Error fetching PR details: {}", e),
    }

    // Check for new reviews
    match client.get_pr_reviews(pr.number).await {
        Ok(reviews) => {
            for review in reviews {
                if !state.seen_reviews.contains(&review.id)
                    && review.submitted_at > state.start_time
                {
                    println!("New review found: {}", review.id);
                    let event = match review.state {
                        ReviewState::Approved => Some(NotificationEvent::PrApproved),
                        ReviewState::ChangesRequested => {
                            Some(NotificationEvent::PrChangesRequested)
                        }
                        _ => None,
                    };
                    if let Some(event) = event {
                        if sender.send(event).is_err() {
                            eprintln!("Failed to send to GUI thread in check_pr_events.");
                            return;
                        }
                    }
                    state.seen_reviews.insert(review.id);
                }
            }
        }
        Err(e) => eprintln!("Error fetching PR reviews: {}", e),
    }

    // Check for new comments
    match client.get_pr_comments(pr.number).await {
        Ok(comments) => {
            for comment in comments {
                if !state.seen_comments.contains(&comment.id)
                    && comment.created_at > state.start_time
                {
                    println!("New comment found: {}", comment.id);
                    if sender.send(NotificationEvent::PrNewComment).is_err() {
                        eprintln!("Failed to send to GUI thread in check_pr_events.");
                        return;
                    }
                    state.seen_comments.insert(comment.id);
                }
            }
        }
        Err(e) => eprintln!("Error fetching PR comments: {}", e),
    }
}
