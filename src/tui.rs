use crate::bitbucket::{BitbucketClient, PullRequest};
use crate::config::{Config, PrStatus, RepoRef};
use anyhow::{Result, anyhow};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Text};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};
use std::io;
use std::str::FromStr;
use std::time::Duration;

#[derive(Clone, Copy)]
struct CommandSpec {
    name: &'static str,
    usage: &'static str,
    accepts_args: bool,
}

const COMMAND_SPECS: [CommandSpec; 7] = [
    CommandSpec {
        name: "/help",
        usage: "show available commands",
        accepts_args: false,
    },
    CommandSpec {
        name: "/repo",
        usage: "add/rm repository entries",
        accepts_args: true,
    },
    CommandSpec {
        name: "/repos",
        usage: "list configured repositories",
        accepts_args: false,
    },
    CommandSpec {
        name: "/status",
        usage: "set status filter",
        accepts_args: true,
    },
    CommandSpec {
        name: "/refresh",
        usage: "reload pull requests",
        accepts_args: false,
    },
    CommandSpec {
        name: "/search",
        usage: "filter PRs by number or text",
        accepts_args: true,
    },
    CommandSpec {
        name: "/quit",
        usage: "exit the app",
        accepts_args: false,
    },
];

pub fn run_app(config: Config) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    let result = run_event_loop(&mut terminal, config);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

fn run_event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    config: Config,
) -> Result<()> {
    let mut app = App::new(config);
    app.log("Type /help for commands.");
    app.refresh_pull_requests();

    loop {
        terminal.draw(|frame| app.draw(frame))?;

        if event::poll(Duration::from_millis(200))?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            app.handle_key(key)?;
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}

struct App {
    config: Config,
    status_filter: PrStatus,
    input: String,
    logs: Vec<String>,
    pull_requests: Vec<PullRequest>,
    all_pull_requests: Vec<PullRequest>,
    search_query: Option<String>,
    selected_index: usize,
    command_suggestion_index: usize,
    should_quit: bool,
}

impl App {
    fn new(config: Config) -> Self {
        let status_filter = config.status();
        Self {
            config,
            status_filter,
            input: String::new(),
            logs: Vec::new(),
            pull_requests: Vec::new(),
            all_pull_requests: Vec::new(),
            search_query: None,
            selected_index: 0,
            command_suggestion_index: 0,
            should_quit: false,
        }
    }

    fn draw(&self, frame: &mut ratatui::Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(6),
                Constraint::Length(8),
                Constraint::Length(3),
            ])
            .split(frame.area());

        let auth_status = if self.config.credentials().is_some() {
            "configured"
        } else {
            "missing"
        };

        let header = Paragraph::new(Text::from(vec![
            Line::from("myprs - Bitbucket PR TUI"),
            Line::from(format!(
                "Repos: {} | Status: {} | API token auth: {}",
                self.config.repos().len(),
                self.status_filter,
                auth_status
            )),
        ]))
        .block(Block::default().borders(Borders::ALL).title("Status"));
        frame.render_widget(header, chunks[0]);

        let list_title = match &self.search_query {
            Some(query) => format!(
                "My Pull Requests ({}) | Search: {}",
                self.status_filter, query
            ),
            None => format!("My Pull Requests ({})", self.status_filter),
        };
        let list_block = Block::default().borders(Borders::ALL).title(list_title);
        if self.pull_requests.is_empty() {
            let empty_state = if let Some(query) = &self.search_query {
                format!("No PRs match search '{query}'. Use /search clear to reset.")
            } else {
                "No pull requests loaded. Configure credentials, add repos, then run /refresh."
                    .to_string()
            };
            frame.render_widget(Paragraph::new(empty_state).block(list_block), chunks[1]);
        } else {
            let (rows, selected_row) = self.grouped_rows();
            let items = rows
                .into_iter()
                .map(|(text, is_header)| {
                    if is_header {
                        ListItem::new(text).style(Style::default().add_modifier(Modifier::BOLD))
                    } else {
                        ListItem::new(text)
                    }
                })
                .collect::<Vec<_>>();

            let list = List::new(items)
                .block(list_block)
                .highlight_style(
                    Style::default()
                        .fg(Color::Yellow)
                        .bg(Color::Black)
                        .add_modifier(Modifier::BOLD),
                )
                .highlight_symbol("> ");

            let mut state = ListState::default();
            state.select(selected_row);
            frame.render_stateful_widget(list, chunks[1], &mut state);
        }

        let log_lines = self
            .logs
            .iter()
            .rev()
            .take(6)
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>();
        frame.render_widget(
            Paragraph::new(log_lines.join("\n"))
                .block(Block::default().borders(Borders::ALL).title("Log")),
            chunks[2],
        );

        let input = Paragraph::new(self.input.as_str()).block(
            Block::default()
                .borders(Borders::ALL)
                .title("Command (/help)"),
        );
        frame.render_widget(input, chunks[3]);
        frame.set_cursor_position((chunks[3].x + self.input.len() as u16 + 1, chunks[3].y + 1));

        let suggestions = self.command_suggestions();
        if !suggestions.is_empty() {
            let popup_height = suggestions.len() as u16 + 2;
            let popup_width = chunks[3].width.min(72);
            let popup_area = Rect::new(
                chunks[3].x,
                chunks[3].y.saturating_sub(popup_height),
                popup_width,
                popup_height,
            );

            let items = suggestions
                .iter()
                .map(|spec| ListItem::new(format!("{:<8} {}", spec.name, spec.usage)))
                .collect::<Vec<_>>();

            let list = List::new(items)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title("Commands (Up/Down + Tab)"),
                )
                .highlight_style(Style::default().add_modifier(Modifier::BOLD))
                .highlight_symbol("> ");

            let mut state = ListState::default();
            let selected = self.command_suggestion_index.min(suggestions.len() - 1);
            state.select(Some(selected));

            frame.render_widget(Clear, popup_area);
            frame.render_stateful_widget(list, popup_area, &mut state);
        }
    }

    fn handle_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.should_quit = true;
            }
            KeyCode::Esc => {
                self.should_quit = true;
            }
            KeyCode::Up => {
                if self.has_command_suggestions() {
                    self.move_command_selection(-1);
                } else {
                    self.selected_index = self.selected_index.saturating_sub(1);
                }
            }
            KeyCode::Down => {
                if self.has_command_suggestions() {
                    self.move_command_selection(1);
                } else if self.selected_index + 1 < self.pull_requests.len() {
                    self.selected_index += 1;
                }
            }
            KeyCode::Tab => {
                let _ = self.apply_command_completion();
            }
            KeyCode::Enter => {
                if self.apply_command_completion_if_partial() {
                    return Ok(());
                }

                let command = self.input.trim().to_string();
                if !command.is_empty() && !command.starts_with("/search") {
                    self.clear_search_filter_if_active();
                }
                self.input.clear();
                if command.is_empty() {
                    if self.pull_requests.is_empty() {
                        self.log("No pull request selected.");
                    } else {
                        let index = self
                            .selected_index
                            .min(self.pull_requests.len().saturating_sub(1))
                            + 1;
                        if let Err(err) = self.open_pull_request(index) {
                            self.log(&format!("Command failed: {err}"));
                        }
                    }
                } else if let Err(err) = self.execute_command(&command) {
                    self.log(&format!("Command failed: {err}"));
                }
            }
            KeyCode::Backspace => {
                self.input.pop();
                self.command_suggestion_index = 0;
            }
            KeyCode::Char(ch) => {
                self.input.push(ch);
                self.command_suggestion_index = 0;
            }
            _ => {}
        }

        Ok(())
    }

    fn execute_command(&mut self, command: &str) -> Result<()> {
        if !command.starts_with('/') {
            self.log("Commands must start with '/'. Try /help.");
            return Ok(());
        }

        let mut parts = command.split_whitespace();
        let name = parts.next().unwrap_or_default();
        let args = parts.collect::<Vec<_>>();

        match name {
            "/help" => {
                self.log("Commands: /repo add <w>/<r>, /repo rm <w>/<r>, /repos, /status <open|merged|declined|all>, /refresh, /search <text|pr-number>, /search clear, /quit");
                self.log(
                    "Tip: type '/' to show command suggestions; use Up/Down + Tab to autocomplete.",
                );
                self.log("Tip: press Enter with empty command input to open selected PR.");
            }
            "/quit" => {
                self.should_quit = true;
            }
            "/repo" => {
                self.handle_repo_command(&args)?;
            }
            "/repos" => {
                self.show_repos();
            }
            "/status" => {
                self.handle_status_command(&args)?;
            }
            "/refresh" => self.refresh_pull_requests(),
            "/search" => self.handle_search_command(&args),
            _ => {
                self.log("Unknown command. Try /help.");
            }
        }

        Ok(())
    }

    fn handle_repo_command(&mut self, args: &[&str]) -> Result<()> {
        if args.is_empty() {
            return Err(anyhow!(
                "usage: /repo add <workspace>/<repo> | /repo rm <workspace>/<repo>"
            ));
        }

        match args[0] {
            "add" => {
                let repo = args
                    .get(1)
                    .ok_or_else(|| anyhow!("usage: /repo add <workspace>/<repo>"))?;
                let repo_ref = RepoRef::parse(repo)?;
                if self.config.add_repo(repo_ref.clone()) {
                    self.config.save()?;
                    self.log(&format!("Added repo {repo_ref}"));
                } else {
                    self.log(&format!("Repo {repo_ref} already exists"));
                }
            }
            "rm" | "remove" => {
                let repo = args
                    .get(1)
                    .ok_or_else(|| anyhow!("usage: /repo rm <workspace>/<repo>"))?;
                let repo_ref = RepoRef::parse(repo)?;
                if self.config.remove_repo(&repo_ref) {
                    self.config.save()?;
                    self.log(&format!("Removed repo {repo_ref}"));
                } else {
                    self.log(&format!("Repo {repo_ref} not found"));
                }
            }
            repo_value => {
                let repo_ref = RepoRef::parse(repo_value)?;
                if self.config.add_repo(repo_ref.clone()) {
                    self.config.save()?;
                    self.log(&format!("Added repo {repo_ref}"));
                } else {
                    self.log(&format!("Repo {repo_ref} already exists"));
                }
            }
        }

        Ok(())
    }

    fn show_repos(&mut self) {
        if self.config.repos().is_empty() {
            self.log("No repos configured. Add one with /repo add <workspace>/<repo>.");
            return;
        }

        self.log("Configured repos:");
        let repo_lines = self
            .config
            .repos()
            .iter()
            .map(|repo| format!("- {repo}"))
            .collect::<Vec<_>>();
        for line in repo_lines {
            self.log(&line);
        }
    }

    fn handle_status_command(&mut self, args: &[&str]) -> Result<()> {
        let value = args
            .first()
            .ok_or_else(|| anyhow!("usage: /status <open|merged|declined|all>"))?;
        let status = PrStatus::from_str(value)?;
        self.status_filter = status;

        if self.config.set_status(status) {
            self.config.save()?;
        }

        self.log(&format!("Status filter set to {}. Refreshing...", status));
        self.refresh_pull_requests();
        Ok(())
    }

    fn refresh_pull_requests(&mut self) {
        let Some((email, api_token)) = self
            .config
            .credentials()
            .map(|(email, token)| (email.to_string(), token.to_string()))
        else {
            self.log("Missing credentials. Set BITBUCKET_EMAIL and BITBUCKET_API_TOKEN.");
            return;
        };

        let repos = self.config.repos().to_vec();
        if repos.is_empty() {
            self.log("No repos configured. Add repos via /repo add <workspace>/<repo>.");
            return;
        }

        let client = BitbucketClient::new(self.config.bitbucket_base_url.clone(), email, api_token);
        let user_uuid = match client.current_user_uuid() {
            Ok(uuid) => uuid,
            Err(err) => {
                self.log(&format!("Failed to fetch current user: {err}"));
                return;
            }
        };

        let mut all_prs = Vec::new();
        let mut failed_repos = 0usize;
        for repo in &repos {
            match client.list_pull_requests_created_by(
                &repo.workspace,
                &repo.repo,
                &user_uuid,
                self.status_filter,
            ) {
                Ok(mut prs) => all_prs.append(&mut prs),
                Err(err) => {
                    failed_repos += 1;
                    self.log(&format!("Failed loading {}: {err}", repo));
                }
            }
        }

        all_prs.sort_by(|left, right| {
            left.workspace
                .cmp(&right.workspace)
                .then(left.repo.cmp(&right.repo))
                .then_with(|| right.updated_on.cmp(&left.updated_on))
        });
        self.selected_index = 0;
        self.all_pull_requests = all_prs;
        self.apply_search_filter();

        if let Some(query) = &self.search_query {
            self.log(&format!(
                "Loaded {} matching PR(s) out of {} total with status '{}' across {} repo(s) | search='{}'",
                self.pull_requests.len(),
                self.all_pull_requests.len(),
                self.status_filter,
                repos.len(),
                query
            ));
        } else {
            self.log(&format!(
                "Loaded {} PR(s) with status '{}' across {} repo(s)",
                self.pull_requests.len(),
                self.status_filter,
                repos.len()
            ));
        }

        if failed_repos > 0 {
            self.log(&format!("{} repo(s) failed during refresh", failed_repos));
        }
    }

    fn open_pull_request(&mut self, index: usize) -> Result<()> {
        if index == 0 {
            return Err(anyhow!("pull request index must be >= 1"));
        }
        let zero_index = index - 1;

        let Some(pr) = self.pull_requests.get(zero_index) else {
            return Err(anyhow!("no pull request at index {index}"));
        };

        webbrowser::open(&pr.url)?;
        self.log(&format!(
            "Opened {}/{} PR #{} in browser.",
            pr.workspace, pr.repo, pr.id
        ));
        Ok(())
    }

    fn handle_search_command(&mut self, args: &[&str]) {
        let query = args.join(" ").trim().to_string();
        if query.is_empty() || query.eq_ignore_ascii_case("clear") {
            self.search_query = None;
            self.apply_search_filter();
            self.log("Search cleared.");
            return;
        }

        self.search_query = Some(query.clone());
        self.apply_search_filter();
        self.log(&format!(
            "Search set to '{query}'. {} matching PR(s).",
            self.pull_requests.len()
        ));
    }

    fn clear_search_filter_if_active(&mut self) {
        if self.search_query.is_none() {
            return;
        }
        self.search_query = None;
        self.apply_search_filter();
        self.log("Search cleared due to non-search command.");
    }

    fn log(&mut self, message: &str) {
        self.logs.push(message.to_string());
    }

    fn grouped_rows(&self) -> (Vec<(String, bool)>, Option<usize>) {
        let mut rows = Vec::new();
        let mut selected_row = None;
        let mut current_repo: Option<String> = None;
        let mut repo_pr_index = 0usize;
        let selected_pr_index = self
            .selected_index
            .min(self.pull_requests.len().saturating_sub(1));

        let mut repo_counts = std::collections::HashMap::<String, usize>::new();
        for pr in &self.pull_requests {
            let key = format!("{}/{}", pr.workspace, pr.repo);
            *repo_counts.entry(key).or_insert(0) += 1;
        }

        for (pr_index, pr) in self.pull_requests.iter().enumerate() {
            let repo_key = format!("{}/{}", pr.workspace, pr.repo);
            if current_repo.as_deref() != Some(repo_key.as_str()) {
                repo_pr_index = 0;
                let count = repo_counts.get(&repo_key).copied().unwrap_or(0);
                let label = if count == 1 { "PR" } else { "PRs" };
                rows.push((format!("{} ({} {}):", repo_key, count, label), true));
                current_repo = Some(repo_key);
            }

            if pr_index == selected_pr_index {
                selected_row = Some(rows.len());
            }

            repo_pr_index += 1;
            rows.push((
                format!(
                    "  {}. #{} [{}] {} ({})",
                    repo_pr_index, pr.id, pr.state, pr.title, pr.author
                ),
                false,
            ));
        }

        (rows, selected_row)
    }

    fn apply_search_filter(&mut self) {
        let Some(raw_query) = self.search_query.as_ref() else {
            self.pull_requests = self.all_pull_requests.clone();
            self.selected_index = self
                .selected_index
                .min(self.pull_requests.len().saturating_sub(1));
            return;
        };

        let query = raw_query.trim().to_ascii_lowercase();
        if query.is_empty() {
            self.pull_requests = self.all_pull_requests.clone();
            self.selected_index = self
                .selected_index
                .min(self.pull_requests.len().saturating_sub(1));
            return;
        }

        self.pull_requests = self
            .all_pull_requests
            .iter()
            .filter(|pr| {
                if pr.id.to_string().contains(&query) {
                    return true;
                }

                let searchable = format!("{} {}", pr.title, pr.description).to_ascii_lowercase();
                searchable.contains(&query)
            })
            .cloned()
            .collect();

        self.selected_index = self
            .selected_index
            .min(self.pull_requests.len().saturating_sub(1));
    }

    fn command_query(&self) -> Option<&str> {
        let trimmed = self.input.trim_start();
        if !trimmed.starts_with('/') {
            return None;
        }
        if trimmed.ends_with(' ') {
            return None;
        }

        let mut parts = trimmed.split_whitespace();
        let command = parts.next().unwrap_or_default();
        if parts.next().is_some() {
            return None;
        }

        Some(command)
    }

    fn command_suggestions(&self) -> Vec<CommandSpec> {
        let Some(query) = self.command_query() else {
            return Vec::new();
        };

        COMMAND_SPECS
            .iter()
            .copied()
            .filter(|spec| spec.name.starts_with(query))
            .collect()
    }

    fn has_command_suggestions(&self) -> bool {
        !self.command_suggestions().is_empty()
    }

    fn move_command_selection(&mut self, direction: i32) {
        let suggestions = self.command_suggestions();
        if suggestions.is_empty() {
            self.command_suggestion_index = 0;
            return;
        }

        let len = suggestions.len() as i32;
        let next = (self.command_suggestion_index as i32 + direction).rem_euclid(len);
        self.command_suggestion_index = next as usize;
    }

    fn apply_command_completion(&mut self) -> bool {
        let suggestions = self.command_suggestions();
        if suggestions.is_empty() {
            return false;
        }

        let selected = suggestions[self.command_suggestion_index.min(suggestions.len() - 1)];
        self.input = if selected.accepts_args {
            format!("{} ", selected.name)
        } else {
            selected.name.to_string()
        };
        self.command_suggestion_index = 0;
        true
    }

    fn apply_command_completion_if_partial(&mut self) -> bool {
        let Some(query) = self.command_query() else {
            return false;
        };

        let suggestions = self.command_suggestions();
        if suggestions.is_empty() {
            return false;
        }

        let selected = suggestions[self.command_suggestion_index.min(suggestions.len() - 1)];
        if query == selected.name {
            return false;
        }

        self.input = if selected.accepts_args {
            format!("{} ", selected.name)
        } else {
            selected.name.to_string()
        };
        self.command_suggestion_index = 0;
        true
    }
}
