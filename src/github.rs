// src/github.rs

//! Handles all communication with the GitHub API.

use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION, USER_AGENT};
use serde::Deserialize;
use std::env;

// Represents a branch summary from the GitHub API /branches endpoint
#[derive(Deserialize, Debug)]
pub struct BranchInfo {
    pub name: String,
}

#[derive(Deserialize, Debug)]
pub struct PullRequest {
    pub number: u64,
    pub state: String,
    pub merged: bool,
    pub head: Head,
}

#[derive(Deserialize, Debug)]
pub struct Head {
    pub sha: String,
}

#[derive(Deserialize, Debug)]
pub struct Status {
    pub state: String,
}

#[derive(Deserialize, Debug)]
pub struct Review {
    pub state: String,
}

/// Returns a list of all branch names for the given repository.
pub async fn get_all_branch_names(owner: &str, repo: &str) -> Result<Vec<String>, reqwest::Error> {
    let token = env::var("GITHUB_TOKEN").unwrap_or_default();
    let client = reqwest::Client::new();
    let mut headers = HeaderMap::new();
    headers.insert(
        ACCEPT,
        HeaderValue::from_str("application/vnd.github.v3+json").unwrap(),
    );
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {}", token)).unwrap(),
    );
    headers.insert(USER_AGENT, HeaderValue::from_str("reposouls-app").unwrap());

    let branches_url = format!("https://api.github.com/repos/{}/{}/branches", owner, repo);

    let remote_branches: Vec<BranchInfo> = client
        .get(&branches_url)
        .headers(headers)
        .send()
        .await?
        .json()
        .await?;

    Ok(remote_branches.into_iter().map(|b| b.name).collect())
}

pub async fn get_branch_status_image(owner: &str, repo: &str, branch: &str) -> String {
    println!("\n[DEBUG] --------------------------------------------------");
    println!("[DEBUG] Checking status for: {}/{}, branch: {}", owner, repo, branch);

    let token = match env::var("GITHUB_TOKEN") {
        Ok(token) => token,
        Err(_) => {
            eprintln!("[DEBUG] GITHUB_TOKEN not set. Returning failure image.");
            return "images/CI PIPELINE FAILED.png".to_string();
        }
    };

    let client = reqwest::Client::new();
    let mut headers = HeaderMap::new();
    headers.insert(
        ACCEPT,
        HeaderValue::from_str("application/vnd.github.v3+json").unwrap(),
    );
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {}", token)).unwrap(),
    );
    headers.insert(USER_AGENT, HeaderValue::from_str("reposouls-app").unwrap());

    let pr_url = format!(
        "https://api.github.com/repos/{}/{}/pulls?head={}:{}",
        owner, repo, owner, branch
    );

    let prs: Vec<PullRequest> = match client.get(&pr_url).headers(headers.clone()).send().await {
        Ok(res) => match res.json().await {
            Ok(json) => {
                println!("[DEBUG] Found PRs: {:#?}", json);
                json
            }
            Err(e) => {
                println!("[DEBUG] Failed to parse PR JSON: {}. Returning empty.", e);
                return "".to_string();
            }
        },
        Err(e) => {
            println!("[DEBUG] PR request failed: {}. Returning failure image.", e);
            return "images/CI PIPELINE FAILED.png".to_string();
        }
    };

    if let Some(pr) = prs.first() {
        println!("[DEBUG] Processing PR #{}: {}", pr.number, pr.head.sha);
        if pr.merged {
            println!("[DEBUG] Result: PR is merged.");
            return "images/PR MERGE COMPLETED.png".to_string();
        }

        let status_url = format!(
            "https://api.github.com/repos/{}/{}/commits/{}/statuses",
            owner, repo, pr.head.sha
        );
        println!("[DEBUG] Fetching statuses from: {}", status_url);

        let statuses: Vec<Status> = match client.get(&status_url).headers(headers.clone()).send().await {
            Ok(res) => {
                let res_json: Vec<Status> = res.json().await.unwrap_or_default();
                println!("[DEBUG] Found statuses: {:#?}", res_json);
                res_json
            }
            Err(e) => {
                println!("[DEBUG] Status request failed: {}", e);
                vec![]
            }
        };

        if let Some(status) = statuses.first() {
            println!("[DEBUG] Latest status state: '{}'", status.state);
            match status.state.as_str() {
                "failure" | "error" => {
                    println!("[DEBUG] Result: Detected CI failure/error.");
                    return "images/CI PIPELINE FAILED.png".to_string();
                }
                "success" => { /* Continue */ }
                _ => {} // Pending or other states
            }
        }

        let review_url = format!(
            "https://api.github.com/repos/{}/{}/pulls/{}/reviews",
            owner, repo, pr.number
        );
        println!("[DEBUG] Fetching reviews from: {}", review_url);
        let reviews: Vec<Review> = match client.get(&review_url).headers(headers).send().await {
            Ok(res) => {
                let res_json: Vec<Review> = res.json().await.unwrap_or_default();
                println!("[DEBUG] Found reviews: {:#?}", res_json);
                res_json
            }
            Err(e) => {
                println!("[DEBUG] Review request failed: {}", e);
                vec![]
            }
        };

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
                        println!("[DEBUG] Result: New comment detected.");
                        return "images/PR NEW COMMENT APPEARED.png".to_string();
                    }
                }
                _ => {} // Ignore other states like DISMISSED
            }
        }

        if has_approved {
            println!("[DEBUG] Result: PR is approved.");
            return "images/PR APPROVAL GRANTED.png".to_string();
        }
        if has_changes_requested {
            println!("[DEBUG] Result: Changes requested.");
            return "images/PR CHANGES REQUIRED.png".to_string();
        }

        if let Some(status) = statuses.first() {
            if status.state == "success" {
                println!("[DEBUG] Result: CI is green (no definitive review).");
                return "images/CI PIPELINE GREENED.png".to_string();
            }
        }
    }

    println!("[DEBUG] Result: No specific status matched. Returning empty.");
    "".to_string()
}