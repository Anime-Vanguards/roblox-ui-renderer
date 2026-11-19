# Assets, cookies, and Studio image reads

This page covers where `rur` loads textures, fonts, and thumbnails from, how authentication works for private assets (an Open Cloud API key, a cookie, or an account broker), and how to read private game images directly from an open Roblox Studio.

## Asset sources and caching

- **Built-in assets (`rbxasset://`):** Loaded from your local Roblox Studio installation (override with `RUR_ROBLOX_CONTENT`).
- **Uploaded assets (`rbxassetid://`):** Downloaded by `rur-native` (`native/src/download.rs`) from Roblox asset delivery and cached in `<state dir>/assets/<id>.<ext>` alongside a download record. The public route is tried first; for an asset it refuses as private (401 or 403), the [Open Cloud API key](#open-cloud-api-key) comes next, then the [cookie](#roblox-cookie-roblosecurity) or [broker](#an-account-broker-instead-of-the-cookie). Files without a valid record are re-downloaded before use. Pass `--offline` to disable network access. Never drop placeholder files into the cache—use `file://` URIs for local images instead.
- **Thumbnails (`rbxthumb://`):** Fetched as WebP from Roblox's thumbnail service (matching Studio), decoded, and cached as PNG.
- **CLI helpers:** `rur-native assets fetch <uri>...` (`--offline`, `--json`) resolves and caches URIs using the exact render path; `rur-native assets account` reports which credentials are active (`api key, then cookie`, `broker`, `none`...; `--json`: `{"access": "broker" | "cookie" | null, "apiKey": true | false}`), never their values.

### Open Cloud API key

An [Open Cloud API key](https://create.roblox.com/docs/cloud/auth/api-keys) with asset read access is the narrowest way to download private assets: it reads assets, nothing else about the account. Configure it from one of:
1. The `ROBLOX_ASSET_DELIVERY_API_KEY` environment variable
2. A `ROBLOX_ASSET_DELIVERY_API_KEY=...` line in the file named by `RUR_COOKIE_ENV` (written like the cookie there: quotes and `#` comments work)

A source checkout's `.env` is not read for the key. A value with a space, a control character, or a character outside ASCII is not used (`rur-native doctor` says so, without the value).

- **Where it goes:** only in the `x-api-key` header, only over HTTPS to `https://apis.roblox.com/asset-delivery-api/v1/assetId/<id>`, and only after the public route answered 401 or 403. Its answer's CDN location is downloaded without it, and a redirect is never followed with it.
- **Order:** before the cookie or broker. If Open Cloud also refuses the asset, the cookie or broker is asked next, and the error lists each answer.
- **One request per asset:** Open Cloud has no batch route. With a key, the batch lookup goes without the cookie, so every private asset is asked with the key first.
- **Rate limit** (1000 requests a minute per key owner): a `429` waits for its `Retry-After` (else `x-ratelimit-reset`, else 1, 2, 4, 8 s) and retries, up to 5 requests per asset. An answer whose `x-ratelimit-remaining` is 0 holds every request with the key until `x-ratelimit-reset`. No single wait is longer than a minute.
- **Kept private:** never printed, logged, cached, or written to file names, records, recordings or `rur-native debug-report` (which also replaces it with `<api key>`, should it ever reach a message). `rur-native` takes the variable out of its own environment at startup, so no child process inherits it: not the ViewportFrame worker, git, ffmpeg, Node, or a process the engine starts. Its own render processes (batch jobs, `debug-report` and `sweep` renders) get it back. The Node CLI strips it from every child that doesn't download (`withoutCookie`).

### Roblox cookie (`.ROBLOSECURITY`)

For assets that Roblox's public asset delivery rejects, configure a `.ROBLOSECURITY` cookie from one of:
1. `ROBLOX_ASSET_DELIVERY_COOKIE` or `ROBLOSECURITY` environment variables
2. The file named by `RUR_COOKIE_ENV`
3. `.env` in a source checkout (copy `.env.example`; **release binaries never read `.env`**)

The cookie is sent exclusively to `https://assetdelivery.roblox.com`, never logged or cached, and stripped from child processes that don't download assets. With an [Open Cloud API key](#open-cloud-api-key) configured, the key is asked first and the cookie only for what the key can't read. Use a dedicated account ([SECURITY.md](../SECURITY.md)), the narrower API key, or an [account broker](#an-account-broker-instead-of-the-cookie) so no developer holds the cookie.


## An account broker instead of the cookie

Teams sharing a single asset account can run an account broker that holds the Roblox session and proxies authenticated asset-delivery calls using personal bearer tokens. Whenever broker credentials are configured, `rur` ignores any local cookie. An Open Cloud API key, when configured, is still asked before the broker.

Lookup priority:

| Priority | Source | Format |
|---|---|---|
| 1 | `ROBLOX_BROKER_URL` + `ROBLOX_BROKER_TOKEN` | Environment variables |
| 2 | `ROBLOX_BROKER_CREDENTIALS` | Path to JSON file `{ "url": "https://...", "token": "..." }` (extra keys ignored) |
| 3 | `~/.config/roblox-broker/credentials.json` | Default JSON file under your user profile |

**Broker behavior:**
- Authenticated requests for `https://assetdelivery.roblox.com/<route>` (`GET /v1/assetId/<id>` and `POST /v1/assets/batch`) go to `<url>/rbx/assetdelivery.roblox.com/<route>` with `Authorization: Bearer <token>`. The broker attaches the Roblox cookie and handles CSRF challenges.
- Unauthenticated initial attempts, CDN asset downloads, and thumbnails still go directly to Roblox without the bearer token. The token is sent only to the broker, never across redirects, never logged or cached, and stripped from non-downloading child processes.
- URLs must use `https://`, or `http://` only for `127.0.0.1` / `localhost`. Other URLs are rejected without falling back to a cookie.
- Broker-generated errors (`X-Broker-Error` header + JSON `message`) surface directly in asset diagnostics.
- `rur doctor` reports active auth (`assetCookie`) as broker, cookie, or none, and the Open Cloud API key (`assetApiKey`) as configured or not.

## Images from Studio

When a Studio MCP bridge is configured and asset delivery refuses a private `rbxassetid://` image, `rur` can read the pixels straight out of an open Roblox Studio—no Roblox cookie required.

- `rur-native` invokes `node cli/studio-images.mjs store` from a source checkout (release binaries need `RUR_ASSET_REPO` set to a checkout with `npm install` run).
- Studio opens images in batches of 12 via `AssetService:CreateEditableImageAsync`, returns zstd-compressed pixels (~2 MB per batch), and `rur` caches them losslessly as `<state dir>/assets/<id>.png` (`"source": "studio"`). Later renders read from cache without touching Studio; `--offline` never queries Studio.
- **Security:** `RUR_STUDIO_MCP_URL` must target loopback (`127.0.0.0/8`, `::1`, or `localhost`), redirects are rejected, and the Luau snippet is strictly read-only.
- Studio can only load assets owned by the open place's creator. If Studio also lacks permission, diagnostics show both asset delivery's error and Studio's reason.

| Variable | Description |
|---|---|
| `RUR_STUDIO_MCP_PORT` | Local TCP MCP bridge port used by `rur compare` ([studio.md](studio.md)) |
| `RUR_STUDIO_MCP_URL` | Local HTTP bridge URL (e.g., `http://127.0.0.1:58741`). Probes `GET /health` to auto-detect `robloxstudio-mcp` 3 (uses `RUR_STUDIO_MCP_TOKEN`) vs. `--http-only` (`GET /instances` + `POST /mcp/execute_luau`). |
| `RUR_STUDIO_MCP_TOKEN` | Auth token for `robloxstudio-mcp` 3 (`ROBLOX_STUDIO_AUTH_TOKEN` or token file content). Sent only in `X-MCP-Auth` to the loopback bridge; never logged or cached. |
| `RUR_STUDIO` | Target Studio ID or window title substring (default: checks each edit session in turn). |
| `RUR_STUDIO_IMAGES=0` | Disable Studio image fallback even when a bridge is configured. |


### Pre-warming the cache (`rur assets warm`)

`rur assets warm` scans the open Studio place for every GUI image (`Image`, `HoverImage`, `PressedImage` on `ImageLabel`/`ImageButton`, and `ScrollingFrame` bar images), downloads public ones via asset delivery, and reads private ones from Studio. Subsequent renders run offline with zero Studio queries. Pass `--refresh` to re-read images previously cached from Studio.

For 3D `ViewportFrame` assets and rendering, see [viewports.md](viewports.md).
