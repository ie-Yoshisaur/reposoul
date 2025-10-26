use reqwest::header::{ACCEPT, AUTHORIZATION, HeaderMap, HeaderValue, USER_AGENT};
use serde::Deserialize;
use std::env;

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
    let token = match env::var("GITHUB_TOKEN") {
        Ok(token) => token,
        Err(_) => {
            eprintln!("Error: GITHUB_TOKEN environment variable not set.");
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
            Ok(json) => json,
            Err(_) => return "".to_string(),
        },
        Err(_) => return "images/CI PIPELINE FAILED.png".to_string(),
    };

    if let Some(pr) = prs.first() {
        if pr.merged {
            return "images/PR MERGE COMPLETED.png".to_string();
        }

        let status_url = format!(
            "https://api.github.com/repos/{}/{}/commits/{}/statuses",
            owner, repo, pr.head.sha
        );

        let statuses: Vec<Status> = match client
            .get(&status_url)
            .headers(headers.clone())
            .send()
            .await
        {
            Ok(res) => res.json().await.unwrap_or_default(),
            Err(_) => vec![],
        };

        if let Some(status) = statuses.first() {
            match status.state.as_str() {
                "failure" | "error" => return "images/CI PIPELINE FAILED.png".to_string(),
                "success" => {}
                _ => {}
            }
        }

        let reviews: Vec<Review> = match client
            .get(&format!(
                "https://api.github.com/repos/{}/{}/pulls/{}/reviews",
                owner, repo, pr.number
            ))
            .headers(headers)
            .send()
            .await
        {
            Ok(res) => res.json().await.unwrap_or_default(),
            Err(_) => vec![],
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
                        return "images/PR NEW COMMENT APPEARED.png".to_string();
                    }
                }
                _ => {}
            }
        }

        if has_approved {
            return "images/PR APPROVAL GRANTED.png".to_string();
        }
        if has_changes_requested {
            return "images/PR CHANGES REQUIRED.png".to_string();
        }

        if let Some(status) = statuses.first() {
            if status.state == "success" {
                return "images/CI PIPELINE GREENED.png".to_string();
            }
        }
    }

    // Default image if no PR or specific status is found
    "".to_string()
}
