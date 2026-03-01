# myprs

`myprs` is a terminal UI for viewing Bitbucket pull requests authored by you across multiple repositories.

It uses Bitbucket API token authentication via HTTP Basic auth (`email + api token`) and stores local settings in `~/.config/myprs/config.toml`.

## UI Preview

![myprs TUI screenshot](./Bitbucket-PR-TUI.png)

## Features

- Fetch PRs you created across one or many repos.
- Group PRs by repository in the main list.
- Filter by PR status: `open`, `merged`, `declined`, `all`.
- Search loaded PRs by PR number or text in title/description.
- Manage repositories directly from the TUI.
- Open selected PR in your browser from the TUI.

## Requirements

- macOS, Linux, or another terminal environment supported by `crossterm`.
- Bitbucket account email and API token.

## Quick Start

1. Install via Homebrew:

```bash
brew install shaunwen/tap/myprs
```

2. Set credentials:

```bash
export BITBUCKET_EMAIL="you@company.com"
export BITBUCKET_API_TOKEN="<atlassian-api-token>"
```

3. Set repositories (recommended):

```bash
export BITBUCKET_REPOS="workspace-a/repo-1,workspace-b/repo-2"
```

4. Run:

```bash
myprs
```

## CLI Options

You can override or add settings at startup:

```bash
myprs \
  --email you@company.com \
  --api-token <token> \
  --repo workspace-a/repo-1 \
  --repo workspace-b/repo-2 \
  --status open
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

## Release Binary (macOS ARM64)

This repository includes a GitHub Actions workflow that builds and uploads a macOS ARM64 binary on tag pushes:

- Workflow file: `.github/workflows/release-macos-arm64.yml`
- Trigger: push tag like `v0.1.1`
- Output asset: `myprs-v0.1.1-aarch64-apple-darwin.tar.gz` and matching `.sha256`

Release flow:

```bash
git tag -a v0.1.1 -m "v0.1.1"
git push origin v0.1.1
```

To verify architecture from a downloaded asset:

```bash
tar -xzf myprs-v0.1.1-aarch64-apple-darwin.tar.gz
file myprs
```

Expected output includes `arm64`.
