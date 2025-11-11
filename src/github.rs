use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::Deserialize;

const GITHUB_API_BASE: &str = "https://api.github.com";

/// A client for interacting with the GitHub API.
#[derive(Debug)]
pub struct GitHubClient {
    /// The HTTP client used to make requests to the GitHub API.
    client: Client,
    /// The owner of the repository.
    owner: String,
    /// The name of the repository.
    repo: String,
    /// The personal access token used to authenticate with the GitHub API.
    token: String,
}

/// Represents a single workflow run in GitHub Actions.
#[derive(Deserialize, Debug)]
pub struct WorkflowRun {
    /// The unique identifier for the workflow run.
    pub id: i64,
    /// The current status of the workflow run.
    pub status: WorkflowRunStatus,
    /// The conclusion of the workflow run.
    pub conclusion: Option<WorkflowRunConclusion>,
    /// The timestamp of when the workflow run was created.
    pub created_at: DateTime<Utc>,
    /// The timestamp of when the workflow run was last updated.
    pub updated_at: DateTime<Utc>,
}

/// Represents the status of a workflow run.
/// See: https://docs.github.com/en/rest/actions/workflow-runs?apiVersion=2022-11-28#get-a-workflow-run
#[derive(Deserialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowRunStatus {
    Completed,
    ActionRequired,
    Cancelled,
    Failure,
    Neutral,
    Skipped,
    Stale,
    Success,
    TimedOut,
    InProgress,
    Queued,
    Requested,
    Waiting,
    Pending,
}

/// Represents the conclusion of a workflow run.
/// See: https://docs.github.com/en/rest/actions/workflow-runs?apiVersion=2022-11-28#get-a-workflow-run
#[derive(Deserialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowRunConclusion {
    Success,
    Failure,
    Cancelled,
    ActionRequired,
    Neutral,
    Skipped,
    Stale,
    TimedOut,
}

/// A list of workflow runs.
#[derive(Deserialize, Debug)]
pub struct ListWorkflowRuns {
    /// A vector containing the workflow runs.
    pub workflow_runs: Vec<WorkflowRun>,
}

/// Represents a pull request on GitHub.
#[derive(Deserialize, Debug)]
pub struct PullRequest {
    /// The unique identifier for the pull request.
    pub id: i64,
    /// The pull request number.
    pub number: u64,
    /// The title of the pull request.
    pub title: String,
    /// Whether the pull request has been merged.
    pub merged: Option<bool>,
    /// The timestamp of when the pull request was merged.
    pub merged_at: Option<DateTime<Utc>>,
    /// The timestamp of when the pull request was created.
    pub created_at: DateTime<Utc>,
    /// The timestamp of when the pull request was last updated.
    pub updated_at: DateTime<Utc>,
}

/// Represents a comment on a pull request.
#[derive(Deserialize, Debug)]
pub struct Comment {
    /// The unique identifier for the comment.
    pub id: i64,
    /// The body of the comment.
    pub body: String,
    /// The timestamp of when the comment was created.
    pub created_at: DateTime<Utc>,
}

/// Represents a review on a pull request.
#[derive(Deserialize, Debug)]
pub struct Review {
    /// The unique identifier for the review.
    pub id: i64,
    /// The current state of the review.
    pub state: ReviewState,
    /// The timestamp of when the review was submitted.
    pub submitted_at: DateTime<Utc>,
}

/// Represents the state of a pull request review.
/// The state can be one of several predefined values.
/// See: https://docs.github.com/en/rest/pulls/reviews?apiVersion=2022-11-28#list-reviews-for-a-pull-request
#[derive(Deserialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReviewState {
    /// The reviewer has approved the pull request.
    Approved,
    /// The reviewer has requested changes to the pull request.
    ChangesRequested,
    /// The reviewer has commented on the pull request.
    Commented,
    /// The reviewer has dismissed the review.
    Dismissed,
    /// The review is pending submission.
    Pending,
}

impl GitHubClient {
    /// Creates a new `GitHubClient`.
    ///
    /// # Arguments
    ///
    /// * `owner` - The owner of the repository.
    /// * `repo` - The name of the repository.
    /// * `token` - The personal access token used to authenticate with the GitHub API.
    pub fn new(owner: String, repo: String, token: String) -> Self {
        Self {
            client: Client::new(),
            owner,
            repo,
            token,
        }
    }

    /// Sends a GET request to the GitHub API and deserializes the response.
    ///
    /// # Arguments
    ///
    /// * `url` - The URL to send the GET request to.
    async fn get<T: for<'de> Deserialize<'de>>(&self, url: &str) -> Result<T, String> {
        let response = self
            .client
            .get(url)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("User-Agent", "reposouls-app")
            .header("Accept", "application/vnd.github.v3+json")
            .send()
            .await
            .map_err(|e| e.to_string())?;

        if response.status().is_success() {
            response
                .json::<T>()
                .await
                .map_err(|e| format!("JSON decode error: {} on URL: {}", e, url))
        } else {
            let status = response.status();
            let text = response
                .text()
                .await
                .unwrap_or_else(|_| "Could not read error body".to_string());
            Err(format!("API Error ({}): {}", status, text))
        }
    }

    /// Gets workflow runs for a specific branch created after a given time.
    ///
    /// # Arguments
    ///
    /// * `branch` - The name of the branch to get workflow runs for.
    /// * `start_time` - The time to fetch workflow runs created after.
    pub async fn get_workflow_runs_for_branch(
        &self,
        branch: &str,
        start_time: DateTime<Utc>,
    ) -> Result<Vec<WorkflowRun>, String> {
        let created_filter = start_time.to_rfc3339();
        let url = format!(
            "{}/repos/{}/{}/actions/runs?branch={}&created=>{}",
            GITHUB_API_BASE, self.owner, self.repo, branch, created_filter
        );
        let response: ListWorkflowRuns = self.get(&url).await?;
        Ok(response.workflow_runs)
    }

    /// Gets the latest pull request for a specific branch.
    ///
    /// # Arguments
    ///
    /// * `branch` - The name of the branch to get the pull request for.
    pub async fn get_pr_for_branch(&self, branch: &str) -> Result<Option<PullRequest>, String> {
        let head = format!("{}:{}", self.owner, branch);
        let url = format!(
            "{}/repos/{}/{}/pulls?state=all&sort=created&direction=desc&head={}&per_page=1",
            GITHUB_API_BASE, self.owner, self.repo, head
        );
        let mut prs: Vec<PullRequest> = self.get(&url).await?;
        Ok(prs.pop())
    }

    /// Gets all comments for a specific pull request.
    /// These are "Issue comments" that appear in the conversation tab.
    ///
    /// # Arguments
    ///
    /// * `pr_number` - The number of the pull request.
    pub async fn get_pr_comments(&self, pr_number: u64) -> Result<Vec<Comment>, String> {
        let url = format!(
            "{}/repos/{}/{}/issues/{}/comments",
            GITHUB_API_BASE, self.owner, self.repo, pr_number
        );
        self.get(&url).await
    }

    /// Gets all reviews for a specific pull request.
    /// These include approvals, change requests, and review comments.
    ///
    /// # Arguments
    ///
    /// * `pr_number` - The number of the pull request.
    pub async fn get_pr_reviews(&self, pr_number: u64) -> Result<Vec<Review>, String> {
        let url = format!(
            "{}/repos/{}/{}/pulls/{}/reviews",
            GITHUB_API_BASE, self.owner, self.repo, pr_number
        );
        self.get(&url).await
    }

    /// Gets the details for a specific pull request.
    ///
    /// # Arguments
    ///
    /// * `pr_number` - The number of the pull request.
    pub async fn get_pr_details(&self, pr_number: u64) -> Result<PullRequest, String> {
        let url = format!(
            "{}/repos/{}/{}/pulls/{}",
            GITHUB_API_BASE, self.owner, self.repo, pr_number
        );
        self.get(&url).await
    }
}
