use git2::Repository;

#[derive(Debug)]
pub struct GitInfo {
    pub owner: String,
    pub repo: String,
    pub branch: String,
}

pub fn get_git_info() -> Result<GitInfo, String> {
    let repo = Repository::open(".").map_err(|e| format!("Failed to open repository: {}", e))?;

    let branch = get_current_branch(&repo)?;
    let (owner, repo_name) = get_owner_and_repo(&repo)?;

    Ok(GitInfo {
        owner,
        repo: repo_name,
        branch,
    })
}

fn get_current_branch(repo: &Repository) -> Result<String, String> {
    let head = repo
        .head()
        .map_err(|e| format!("Failed to get HEAD: {}", e))?;
    let branch_name = head.shorthand().ok_or("HEAD is not on a branch")?;
    Ok(branch_name.to_string())
}

fn get_owner_and_repo(repo: &Repository) -> Result<(String, String), String> {
    let remote = repo
        .find_remote("origin")
        .map_err(|e| format!("Failed to find remote 'origin': {}", e))?;

    let url = remote.url().ok_or("Remote 'origin' has no URL")?;

    let path = if let Some(stripped) = url.strip_prefix("https://github.com/") {
        stripped
    } else if let Some(stripped) = url.strip_prefix("git@github.com:") {
        stripped
    } else {
        return Err(format!("Unsupported git remote URL format: {}", url));
    };

    let cleaned_path = path.trim();
    let repo_path = cleaned_path.trim_end_matches(".git");

    let parts: Vec<&str> = repo_path.split('/').collect();
    if parts.len() == 2 {
        Ok((parts[0].to_string(), parts[1].to_string()))
    } else {
        Err(format!(
            "Could not parse owner and repo from path: {}",
            repo_path
        ))
    }
}
