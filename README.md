# myprs

`myprs` is a terminal UI for viewing Bitbucket pull requests authored by you across multiple repositories.

It uses Bitbucket API token authentication via HTTP Basic auth (`email + api token`) and stores local settings in `~/.config/myprs/config.toml`.

## Features

- Fetch PRs you created across one or many repos.
- Group PRs by repository in the main list.
- Filter by PR status: `open`, `merged`, `declined`, `all`.
- Search loaded PRs by PR number or text in title/description.
- Manage repositories directly from the TUI.
- Open selected PR in your browser from the TUI.

## Requirements

- macOS, Linux, or another terminal environment supported by `crossterm`.
- Recent Rust toolchain with Cargo installed.
- Bitbucket account email and API token.

## Quick Start

1. Set credentials:

```bash
export BITBUCKET_EMAIL="you@company.com"
export BITBUCKET_API_TOKEN="<atlassian-api-token>"
```

2. Set repositories (recommended):

```bash
export BITBUCKET_REPOS="workspace-a/repo-1,workspace-b/repo-2"
```

3. Run:

```bash
cargo run
```

## Environment Variables

Required:

- `BITBUCKET_EMAIL`
- `BITBUCKET_API_TOKEN`

Repository sources:

- `BITBUCKET_REPOS` (comma-separated `workspace/repo` list, preferred)
- `BITBUCKET_WORKSPACE` + `BITBUCKET_REPO` (single-repo compatibility path)

Optional:

- `BITBUCKET_PR_STATUS` (`open|merged|declined|all`)
- `BITBUCKET_BASE_URL` (default: `https://api.bitbucket.org/2.0`)

## CLI Options

You can override or add settings at startup:

```bash
cargo run -- \
  --email you@company.com \
  --api-token <token> \
  --repo workspace-a/repo-1 \
  --repo workspace-b/repo-2 \
  --status open \
  --base-url https://api.bitbucket.org/2.0
```

## TUI Commands

- `/help`
- `/repo add <workspace>/<repo>`
- `/repo rm <workspace>/<repo>`
- `/repos`
- `/status <open|merged|declined|all>`
- `/refresh`
- `/search <text|pr-number>`
- `/search clear`
- `/quit`

## Keybindings

- `Up` / `Down`: move selection (or command suggestion selection in command mode)
- `Tab`: apply selected command suggestion
- `Enter` on empty command input: open selected PR in browser
- `Esc` or `Ctrl+C`: quit

## Example `config.toml`

`myprs` persists runtime config to `~/.config/myprs/config.toml`.

```toml
bitbucket_email = "you@company.com"
bitbucket_api_token = "<atlassian-api-token>"
default_status = "open"

[[repos]]
workspace = "workspace-a"
repo = "repo-1"

[[repos]]
workspace = "workspace-b"
repo = "repo-2"
```
