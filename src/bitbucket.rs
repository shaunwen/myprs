use crate::config::PrStatus;
use anyhow::{Context, Result};
use reqwest::Url;
use reqwest::blocking::{Client, RequestBuilder};
use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct PullRequest {
    pub workspace: String,
    pub repo: String,
    pub id: u64,
    pub title: String,
    pub description: String,
    pub author: String,
    pub state: String,
    pub updated_on: String,
    pub url: String,
}

pub struct BitbucketClient {
    http: Client,
    base_url: String,
    email: String,
    api_token: String,
}

impl BitbucketClient {
    pub fn new(base_url: String, email: String, api_token: String) -> Self {
        Self {
            http: Client::new(),
            base_url,
            email,
            api_token,
        }
    }

    pub fn current_user_uuid(&self) -> Result<String> {
        let endpoint = Url::parse(&format!("{}/user", self.base_url.trim_end_matches('/')))
            .context("failed to build current-user endpoint")?;

        let payload: UserResponse = self
            .auth_get(endpoint)
            .send()
            .context("failed to call Bitbucket user API")?
            .error_for_status()
            .context("Bitbucket user API returned an error status")?
            .json()
            .context("failed to deserialize Bitbucket user response")?;

        Ok(payload.uuid)
    }

    pub fn list_pull_requests_created_by(
        &self,
        workspace: &str,
        repo: &str,
        author_uuid: &str,
        status: PrStatus,
    ) -> Result<Vec<PullRequest>> {
        let mut endpoint = Url::parse(&format!(
            "{}/repositories/{}/{}/pullrequests",
            self.base_url.trim_end_matches('/'),
            workspace,
            repo
        ))
        .context("failed to build Bitbucket pull request endpoint")?;

        let query = build_query(author_uuid, status);
        endpoint
            .query_pairs_mut()
            .append_pair("sort", "-updated_on")
            .append_pair("pagelen", "50")
            .append_pair("q", &query);

        let payload: PullRequestListResponse = self
            .auth_get(endpoint)
            .send()
            .context("failed to call Bitbucket pull request API")?
            .error_for_status()
            .with_context(|| {
                format!("Bitbucket pull request API returned an error for {workspace}/{repo}")
            })?
            .json()
            .context("failed to deserialize Bitbucket pull request response")?;

        Ok(payload
            .values
            .into_iter()
            .map(|value| {
                let description = value
                    .description
                    .or_else(|| value.summary.and_then(|summary| summary.raw))
                    .unwrap_or_default();

                PullRequest {
                    workspace: workspace.to_string(),
                    repo: repo.to_string(),
                    id: value.id,
                    title: value.title,
                    description,
                    author: value
                        .author
                        .display_name
                        .or(value.author.nickname)
                        .unwrap_or_else(|| "unknown".to_string()),
                    state: value.state,
                    updated_on: value.updated_on,
                    url: value.links.html.href,
                }
            })
            .collect())
    }

    fn auth_get(&self, endpoint: Url) -> RequestBuilder {
        self.http
            .get(endpoint)
            .basic_auth(&self.email, Some(&self.api_token))
    }
}

fn build_query(author_uuid: &str, status: PrStatus) -> String {
    let mut terms = vec![format!("author.uuid=\"{}\"", author_uuid)];
    if let Some(state) = status.as_query_state() {
        terms.push(format!("state=\"{}\"", state));
    }
    terms.join(" AND ")
}

#[derive(Debug, Deserialize)]
struct UserResponse {
    uuid: String,
}

#[derive(Debug, Deserialize)]
struct PullRequestListResponse {
    values: Vec<PullRequestValue>,
}

#[derive(Debug, Deserialize)]
struct PullRequestValue {
    id: u64,
    title: String,
    description: Option<String>,
    summary: Option<PullRequestSummary>,
    state: String,
    updated_on: String,
    author: PullRequestAuthor,
    links: PullRequestLinks,
}

#[derive(Debug, Deserialize)]
struct PullRequestSummary {
    raw: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PullRequestAuthor {
    display_name: Option<String>,
    nickname: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PullRequestLinks {
    html: PullRequestHtmlLink,
}

#[derive(Debug, Deserialize)]
struct PullRequestHtmlLink {
    href: String,
}
