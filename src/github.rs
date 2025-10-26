// src/github.rs

//! Handles all communication with the GitHub API.

use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION, USER_AGENT};
use serde::Deserialize;
use std::env;
use std::error::Error;

// --- Data Structures for GitHub API Responses ---

#[derive(Deserialize, Debug)]
pub struct BranchInfo {
    pub commit: CommitInfo,
}

#[derive(Deserialize, Debug)]
pub struct CommitInfo {
    pub sha: String,
}

#[derive(Deserialize, Debug)]
pub struct PullRequest {
    pub number: u64,
    pub merged: bool,
}

#[derive(Deserialize, Debug)]
pub struct Status {
    pub state: String,
}

#[derive(Deserialize, Debug)]
pub struct CheckRun {
    pub conclusion: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct CheckRunsResponse {
    pub check_runs: Vec<CheckRun>,
}

#[derive(Deserialize, Debug)]
pub struct Review {
    pub state: String,
}

// --- Public Functions ---

/// Returns a list of all branch names for the given repository.
pub async fn get_all_branch_names(owner: &str, repo: &str) -> Result<Vec<String>, Box<dyn Error>> {
    let client = build_client()?;
    let branches_url = format!("https://api.github.com/repos/{}/{}/branches", owner, repo);

    #[derive(Deserialize)]
    struct BranchName { name: String }
    let remote_branches: Vec<BranchName> = client.get(&branches_url).send().await?.json().await?;

    Ok(remote_branches.into_iter().map(|b| b.name).collect())
}

/// Determines the visual status image for a given branch by checking its CI status (Checks and Statuses) and PR state (reviews, merge status).
pub async fn get_branch_status_image(owner: &str, repo: &str, branch: &str) -> String {
    let client = match build_client() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error building client: {}", e);
            return "images/CI PIPELINE FAILED.png".to_string();
        }
    };

    // 1. Get the latest commit SHA for the branch
    let latest_commit_sha = match get_latest_commit_sha(&client, owner, repo, branch).await {
        Some(sha) => sha,
        None => return "".to_string(), // Branch not found, do nothing
    };

    // 2. Check CI status (Checks API is preferred, Statuses API is a fallback)
    let ci_status = get_ci_status(&client, owner, repo, &latest_commit_sha).await;
    if ci_status == CiStatus::Failure {
        return "images/CI PIPELINE FAILED.png".to_string();
    }

    // 3. Find a PR associated with the branch
    let pr_url = format!("https://api.github.com/repos/{}/{}/pulls?head={}:{}", owner, repo, owner, branch);
    let prs: Vec<PullRequest> = match client.get(&pr_url).send().await {
        Ok(res) => res.json().await.unwrap_or_default(),
        Err(_) => vec![],
    };

    // 4. If a PR exists, check for reviews and merge status
    if let Some(pr) = prs.first() {
        if pr.merged {
            return "images/PR MERGE COMPLETED.png".to_string();
        }

        if let Some(review_image) = get_pr_review_status_image(&client, owner, repo, pr.number).await {
            return review_image;
        }
    }

    // 5. If no specific PR status, fall back to CI status
    if ci_status == CiStatus::Success {
        return "images/CI PIPELINE GREENED.png".to_string();
    }

    // Default: No significant status found
    "".to_string()
}

// --- Helper Functions ---

#[derive(PartialEq)]
enum CiStatus { Success, Failure, Pending }

/// Builds a reqwest client with standard GitHub API headers.
fn build_client() -> Result<reqwest::Client, Box<dyn Error>> {
    let token = env::var("GITHUB_TOKEN").map_err(|_| "GITHUB_TOKEN not set")?;
    let mut headers = HeaderMap::new();
    headers.insert(ACCEPT, HeaderValue::from_str("application/vnd.github.v3+json")?);
    headers.insert(AUTHORIZATION, HeaderValue::from_str(&format!("Bearer {}", token))?);
    headers.insert(USER_AGENT, HeaderValue::from_str("reposouls-app")?);
    Ok(reqwest::Client::builder().default_headers(headers).build()?)
}

/// Fetches the latest commit SHA for a given branch.
async fn get_latest_commit_sha(client: &reqwest::Client, owner: &str, repo: &str, branch: &str) -> Option<String> {
    let branch_url = format!("https://api.github.com/repos/{}/{}/branches/{}", owner, repo, branch);
    client.get(&branch_url).send().await.ok()?.json::<BranchInfo>().await.ok().map(|b| b.commit.sha)
}

/// Fetches CI status using both Checks and Statuses APIs.
async fn get_ci_status(client: &reqwest::Client, owner: &str, repo: &str, sha: &str) -> CiStatus {
    // Checks API (preferred)
    let check_runs_url = format!("https://api.github.com/repos/{}/{}/commits/{}/check-runs", owner, repo, sha);
    if let Ok(res) = client.get(&check_runs_url).send().await {
        if let Ok(check_runs_response) = res.json::<CheckRunsResponse>().await {
            if !check_runs_response.check_runs.is_empty() {
                let mut all_success = true;
                for check in check_runs_response.check_runs {
                    match check.conclusion.as_deref() {
                        Some("failure") | Some("timed_out") | Some("cancelled") => return CiStatus::Failure,
                        Some("success") => continue,
                        _ => all_success = false, // Pending or other states
                    }
                }
                return if all_success { CiStatus::Success } else { CiStatus::Pending };
            }
        }
    }

    // Statuses API (fallback)
    let status_url = format!("https://api.github.com/repos/{}/{}/commits/{}/statuses", owner, repo, sha);
    if let Ok(res) = client.get(&status_url).send().await {
        if let Ok(statuses) = res.json::<Vec<Status>>().await {
            if let Some(status) = statuses.first() {
                return match status.state.as_str() {
                    "failure" | "error" => CiStatus::Failure,
                    "success" => CiStatus::Success,
                    _ => CiStatus::Pending,
                };
            }
        }
    }

    CiStatus::Pending // Default if nothing is found
}

/// Fetches PR review status.
async fn get_pr_review_status_image(client: &reqwest::Client, owner: &str, repo: &str, pr_number: u64) -> Option<String> {
    let reviews_url = format!("https://api.github.com/repos/{}/{}/pulls/{}/reviews", owner, repo, pr_number);
    let reviews: Vec<Review> = client.get(&reviews_url).send().await.ok()?.json().await.unwrap_or_default();

    let mut has_changes_requested = false;
    let mut has_approved = false;
    for review in reviews.iter().rev() {
        match review.state.as_str() {
            "APPROVED" => {
                has_approved = true;
                break;
            }
            "CHANGES_REQUESTED" => {
                has_changes_requested = true;
            }
            "COMMENTED" => {
                if !has_approved && !has_changes_requested {
                    return Some("images/PR NEW COMMENT APPEARED.png".to_string());
                }
            }
            _ => {}
        }
    }

    if has_approved {
        return Some("images/PR APPROVAL GRANTED.png".to_string());
    }
    if has_changes_requested {
        return Some("images/PR CHANGES REQUIRED.png".to_string());
    }

    None
}
