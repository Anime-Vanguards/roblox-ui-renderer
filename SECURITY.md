# Security policy

## Reporting a vulnerability

Report security vulnerabilities privately through GitHub's **Security** tab by selecting **Report a vulnerability** ([direct link](https://github.com/Anime-Vanguards/roblox-ui-renderer/security/advisories/new)). Only maintainers can view advisories.

Do not open public issues, pull requests, or discussions for security problems.

Include what you found, steps to reproduce, the commit hash tested, and potential impact. A maintainer will reply within a week and keep you updated until resolved. We will credit you in the advisory unless you prefer to stay anonymous.

## Supported versions

Only the latest commit on `main` receives security fixes. There are no release branches.

## Scope

**In scope:** Code, build scripts, and CI workflows in this repository, including:
- Crafted models, places, scripts, or meshes causing arbitrary code execution, writes outside the output folder, or unintended file reads.
- Cross-site or remote access to the local render service or preview servers.
- Credential or token leakage.

**Out of scope:** The Roblox platform, Roblox accounts or games, and already-public third-party dependency vulnerabilities (report those upstream).

## Credentials

- **Roblox cookie (`.ROBLOSECURITY`):** Used only for `rbxassetid://` assets that public asset delivery refuses. Read solely from `.env` in a source checkout (release binaries never read `.env`), `ROBLOX_ASSET_DELIVERY_COOKIE`, `ROBLOSECURITY`, or the file named by `RUR_COOKIE_ENV`. Sent exclusively to `https://assetdelivery.roblox.com`, never printed, logged, or cached, and stripped from child processes that don't download assets (builds, git, PowerShell, ffmpeg, browsers). Use a dedicated throwaway account.
- **Open Cloud API key:** The narrowest credential for private assets ([docs/assets.md](docs/assets.md#open-cloud-api-key)). Read from `ROBLOX_ASSET_DELIVERY_API_KEY` or the file named by `RUR_COOKIE_ENV`. Sent only in the `x-api-key` header, only over HTTPS to `https://apis.roblox.com/asset-delivery-api/v1/assetId/<id>`, only after public asset delivery refused the asset, and before the cookie or broker. Never sent across redirects, printed, logged, or cached. `rur-native` takes it out of its environment at startup so no child process inherits it; only its own render processes get it back.
- **Account broker:** Keeps cookies off local machines by proxying asset delivery via personal bearer tokens ([docs/assets.md](docs/assets.md#an-account-broker-instead-of-the-cookie)). Read from `ROBLOX_BROKER_URL` + `ROBLOX_BROKER_TOKEN`, `ROBLOX_BROKER_CREDENTIALS`, or `~/.config/roblox-broker/credentials.json`. When configured, no cookie is sent; the bearer token is sent only to the broker over HTTPS (or HTTP to `127.0.0.1`/`localhost`), never across redirects, never logged or cached, and stripped from non-downloading child processes.
- **Studio MCP bridge token:** When reading private images from Studio via `robloxstudio-mcp` 3 ([docs/assets.md](docs/assets.md#images-from-studio)), the auth token is read exclusively from `RUR_STUDIO_MCP_TOKEN`, sent only to the loopback bridge URL in `RUR_STUDIO_MCP_URL`, and never printed, logged, or cached.

Never paste a cookie, token, or key into an issue, PR, or log. If you accidentally expose a `.ROBLOSECURITY` cookie, immediately sign out of all sessions in your Roblox account settings to invalidate it, then let us know so we can remove the post. An exposed Open Cloud API key: delete or regenerate it on the Creator Dashboard.
