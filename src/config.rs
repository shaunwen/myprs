use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs;
use std::path::PathBuf;
use std::str::FromStr;

const DEFAULT_BITBUCKET_BASE_URL: &str = "https://api.bitbucket.org/2.0";
const DEFAULT_AUTO_REFRESH_SECONDS: u64 = 120;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepoRef {
    pub workspace: String,
    pub repo: String,
}

impl RepoRef {
    pub fn new(workspace: String, repo: String) -> Self {
        Self { workspace, repo }
    }

    pub fn parse(value: &str) -> Result<Self> {
        let mut parts = value.split('/');
        let workspace = parts.next().unwrap_or_default().trim();
        let repo = parts.next().unwrap_or_default().trim();

        if workspace.is_empty() || repo.is_empty() || parts.next().is_some() {
            bail!("repo must be in the form workspace/repo")
        }

        Ok(Self::new(workspace.to_string(), repo.to_string()))
    }
}

impl fmt::Display for RepoRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.workspace, self.repo)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, clap::ValueEnum, Default)]
#[serde(rename_all = "lowercase")]
pub enum PrStatus {
    #[default]
    Open,
    Merged,
    Declined,
    All,
}

impl PrStatus {
    pub fn as_query_state(self) -> Option<&'static str> {
        match self {
            Self::Open => Some("OPEN"),
            Self::Merged => Some("MERGED"),
            Self::Declined => Some("DECLINED"),
            Self::All => None,
        }
    }
}

impl fmt::Display for PrStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Open => "open",
            Self::Merged => "merged",
            Self::Declined => "declined",
            Self::All => "all",
        };
        write!(f, "{value}")
    }
}

impl FromStr for PrStatus {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        let value = s.trim().to_ascii_lowercase();
        match value.as_str() {
            "open" => Ok(Self::Open),
            "merged" => Ok(Self::Merged),
            "declined" => Ok(Self::Declined),
            "all" => Ok(Self::All),
            _ => Err(anyhow!(
                "invalid status '{s}'. expected: open|merged|declined|all"
            )),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub bitbucket_base_url: String,
    pub bitbucket_email: Option<String>,
    pub bitbucket_api_token: Option<String>,
    pub repos: Vec<RepoRef>,
    pub default_status: PrStatus,
    pub auto_refresh_seconds: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            bitbucket_base_url: DEFAULT_BITBUCKET_BASE_URL.to_string(),
            bitbucket_email: None,
            bitbucket_api_token: None,
            repos: Vec::new(),
            default_status: PrStatus::Open,
            auto_refresh_seconds: DEFAULT_AUTO_REFRESH_SECONDS,
        }
    }
}

impl Config {
    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;
        if !path.exists() {
            return Ok(Self::default());
        }

        let raw = fs::read_to_string(&path)
            .with_context(|| format!("failed to read config at {}", path.display()))?;
        let parsed = toml::from_str(&raw)
            .with_context(|| format!("failed to parse config at {}", path.display()))?;
        Ok(parsed)
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("failed to create config directory {}", parent.display())
            })?;
        }

        let toml = toml::to_string_pretty(self).context("failed to serialize config")?;
        fs::write(&path, toml)
            .with_context(|| format!("failed to write config at {}", path.display()))?;
        Ok(())
    }

    pub fn config_path() -> Result<PathBuf> {
        let home = dirs::home_dir().context("cannot determine home directory")?;
        Ok(home.join(".config").join("myprs").join("config.toml"))
    }

    pub fn apply_env_and_cli(
        &mut self,
        repos: Vec<String>,
        email: Option<String>,
        api_token: Option<String>,
        status: Option<PrStatus>,
        base_url: Option<String>,
        auto_refresh_seconds: Option<u64>,
    ) -> Result<()> {
        let mut changed = false;

        if let Some(value) = read_env("BITBUCKET_EMAIL") {
            self.bitbucket_email = Some(value);
            changed = true;
        }

        if let Some(value) = read_env("BITBUCKET_API_TOKEN") {
            self.bitbucket_api_token = Some(value);
            changed = true;
        }

        if let Some(value) = read_env("BITBUCKET_PR_STATUS") {
            self.default_status = PrStatus::from_str(&value)?;
            changed = true;
        }

        if let Some(value) = read_env("BITBUCKET_BASE_URL") {
            self.bitbucket_base_url = value;
            changed = true;
        }

        if let Some(value) = read_env("BITBUCKET_AUTO_REFRESH_SECONDS") {
            let parsed =
                parse_auto_refresh_seconds("BITBUCKET_AUTO_REFRESH_SECONDS", value.as_str())?;
            changed |= self.set_auto_refresh_seconds(parsed)?;
        }

        if let Some(value) = read_env("BITBUCKET_REPOS") {
            for repo in parse_repo_list(&value)? {
                changed |= self.add_repo(repo);
            }
        }

        if let (Some(workspace), Some(repo)) =
            (read_env("BITBUCKET_WORKSPACE"), read_env("BITBUCKET_REPO"))
        {
            changed |= self.add_repo(RepoRef::new(workspace, repo));
        }

        if let Some(value) = email {
            self.bitbucket_email = Some(value);
            changed = true;
        }

        if let Some(value) = api_token {
            self.bitbucket_api_token = Some(value);
            changed = true;
        }

        if let Some(value) = status {
            changed |= self.set_status(value);
        }

        if let Some(value) = base_url {
            if self.bitbucket_base_url != value {
                self.bitbucket_base_url = value;
                changed = true;
            }
        }

        if let Some(value) = auto_refresh_seconds {
            changed |= self.set_auto_refresh_seconds(value)?;
        }

        for repo in repos {
            changed |= self.add_repo(RepoRef::parse(&repo)?);
        }

        if changed {
            self.save()?;
        }

        Ok(())
    }

    pub fn credentials(&self) -> Option<(&str, &str)> {
        match (&self.bitbucket_email, &self.bitbucket_api_token) {
            (Some(email), Some(token)) => Some((email.as_str(), token.as_str())),
            _ => None,
        }
    }

    pub fn repos(&self) -> &[RepoRef] {
        &self.repos
    }

    pub fn add_repo(&mut self, repo_ref: RepoRef) -> bool {
        if self.repos.contains(&repo_ref) {
            return false;
        }
        self.repos.push(repo_ref);
        true
    }

    pub fn remove_repo(&mut self, repo_ref: &RepoRef) -> bool {
        let before = self.repos.len();
        self.repos.retain(|repo| repo != repo_ref);
        before != self.repos.len()
    }

    pub fn status(&self) -> PrStatus {
        self.default_status
    }

    pub fn set_status(&mut self, status: PrStatus) -> bool {
        if self.default_status == status {
            return false;
        }
        self.default_status = status;
        true
    }

    pub fn auto_refresh_seconds(&self) -> u64 {
        self.auto_refresh_seconds
    }

    pub fn set_auto_refresh_seconds(&mut self, seconds: u64) -> Result<bool> {
        if seconds == 0 {
            bail!("auto refresh seconds must be >= 1")
        }
        if self.auto_refresh_seconds == seconds {
            return Ok(false);
        }
        self.auto_refresh_seconds = seconds;
        Ok(true)
    }
}

fn read_env(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn parse_repo_list(value: &str) -> Result<Vec<RepoRef>> {
    value
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(RepoRef::parse)
        .collect()
}

fn parse_auto_refresh_seconds(key: &str, value: &str) -> Result<u64> {
    let parsed = value
        .parse::<u64>()
        .map_err(|_| anyhow!("{key} must be a positive integer"))?;
    if parsed == 0 {
        bail!("{key} must be >= 1")
    }
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::{Config, PrStatus, RepoRef};

    #[test]
    fn parses_repo_ref() {
        let repo = RepoRef::parse("team/project").expect("expected valid repo ref");
        assert_eq!(repo.workspace, "team");
        assert_eq!(repo.repo, "project");
    }

    #[test]
    fn rejects_invalid_repo_ref() {
        assert!(RepoRef::parse("team").is_err());
        assert!(RepoRef::parse("team/project/extra").is_err());
        assert!(RepoRef::parse("/").is_err());
    }

    #[test]
    fn parses_status_values() {
        assert_eq!(
            "open".parse::<PrStatus>().expect("open parse"),
            PrStatus::Open
        );
        assert_eq!(
            "MERGED".parse::<PrStatus>().expect("merged parse"),
            PrStatus::Merged
        );
        assert_eq!(
            "declined".parse::<PrStatus>().expect("declined parse"),
            PrStatus::Declined
        );
        assert_eq!("all".parse::<PrStatus>().expect("all parse"), PrStatus::All);
    }

    #[test]
    fn default_auto_refresh_seconds_is_120() {
        let config = Config::default();
        assert_eq!(config.auto_refresh_seconds(), 120);
    }

    #[test]
    fn rejects_zero_auto_refresh_seconds() {
        let mut config = Config::default();
        assert!(config.set_auto_refresh_seconds(0).is_err());
    }
}
