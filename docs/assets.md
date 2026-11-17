# Assets and the cookie

- `rbxasset://` textures and fonts come from the Studio install (`RUR_ROBLOX_CONTENT` picks another
  `content` folder).
- `rbxassetid://` assets are downloaded from Roblox's asset delivery (the public route first) and cached in
  the state dir as `assets/<id>.<ext>`, each with a record of the bytes rur downloaded: a cache file without a
  matching record is downloaded again before it is drawn. `--offline` uses the cache as it is. Never put
  stand-ins into the cache; draw a local image through a `file://` URI instead.
- For assets the public route refuses, copy [`.env.example`](../.env.example) to `.env` and set
  `ROBLOX_ASSET_DELIVERY_COOKIE` to a `.ROBLOSECURITY` value (or set it, or `ROBLOSECURITY`, in the
  environment, or name another file with `RUR_COOKIE_ENV`). The cookie is only sent to
  `assetdelivery.roblox.com`, never printed or written anywhere, and kept out of child processes that
  download nothing. Use an account made for this. See [SECURITY.md](../SECURITY.md). A team can use
  [an account broker](#an-account-broker-instead-of-the-cookie) instead, so nobody holds the cookie.
- An image asset delivery refuses (private to the game's creator) can be read from an open Roblox Studio
  instead, with no cookie: see [Images from Studio](#images-from-studio).
- `rbxthumb://` thumbnails come from the thumbnail service (its WebP rendition, as Studio draws).

## An account broker instead of the cookie

A team that shares one asset account can keep its cookie off everyone's machine: an account broker is a
small service that holds the Roblox session and forwards asset delivery's routes for personal tokens.
With broker credentials rur sends no cookie at all, even when one is configured: each request that would
have carried it goes to the broker instead.

| Source, in this order | Meaning |
|---|---|
| `ROBLOX_BROKER_URL` and `ROBLOX_BROKER_TOKEN` | the broker's address and your token, both in the environment |
| `ROBLOX_BROKER_CREDENTIALS` | or a JSON file `{ "url": "https://...", "token": "..." }` (other keys are ignored) |
| `~/.config/roblox-broker/credentials.json` | or that file under your user folder, when no other is named |

- A request for `https://assetdelivery.roblox.com/<route>` goes to `<url>/rbx/assetdelivery.roblox.com/<route>`
  with `Authorization: Bearer <token>`: the lookup of one asset (`GET /v1/assetId/<id>`) and of many
  (`POST /v1/assets/batch`). The broker adds the cookie and answers Roblox's CSRF challenge itself.
- The public first attempt, the CDN downloads and thumbnails go straight to Roblox as before, without the
  token. The token is sent only to the broker, never across a redirect, never printed or written anywhere,
  and kept out of child processes that download nothing.
- The address must be `https`; plain `http` is accepted only for `127.0.0.1` or `localhost` (a tunnel). Any
  other address is refused, and rur does not fall back to the cookie.
- When the broker itself refuses (an `X-Broker-Error` answer with a JSON `message`: the account is signed
  out, the token is unknown, too many requests), the asset's error carries that message.
- `rur doctor` reports under `assetCookie` whether a broker, a cookie or neither is in use.

## Images from Studio

With a Studio MCP bridge configured, a render's `rbxassetid://` images that asset delivery refuses are read
from Studio (`cli/studio-images.mjs`): Studio opens each one as an `EditableImage`
(`AssetService:CreateEditableImageAsync`) and returns its pixels, and rur stores them losslessly as
`assets/<id>.png`, recorded with `"source": "studio"`. One call reads many images: Studio loads 12 at a
time and returns their pixels zstd-compressed, up to about 2 MB per call. Later renders use the cache and
don't ask Studio again. `--offline` never asks Studio. The bridge is on this machine: a `RUR_STUDIO_MCP_URL`
whose host isn't `127.0.0.0/8`, `::1` or `localhost` is refused before any request, and a redirect is refused,
not followed. The Luau only reads (nothing is mounted, edited or saved). No Roblox credential is used;
robloxstudio-mcp 3 needs a token of its own, below.

| Variable | Meaning |
|---|---|
| `RUR_STUDIO_MCP_PORT` | the TCP bridge `rur compare` uses ([studio.md](studio.md)) |
| `RUR_STUDIO_MCP_URL` | or an HTTP bridge on this machine, for example `http://127.0.0.1:58741`. Its `GET /health` picks the API: robloxstudio-mcp 3 or later uses its own, with `RUR_STUDIO_MCP_TOKEN`; any other answer, or none, uses `GET /instances` and `POST /mcp/execute_luau` (`robloxstudio-mcp --http-only`) |
| `RUR_STUDIO_MCP_TOKEN` | robloxstudio-mcp 3's auth token: the server's `ROBLOX_STUDIO_AUTH_TOKEN`, or the contents of its token file. Read only from this variable, sent only to that bridge (in `X-MCP-Auth`), and never printed, cached or written anywhere |
| `RUR_STUDIO` | the Studio to read from: its id or part of its name (default: each edit session in turn) |
| `RUR_STUDIO_IMAGES=0` | don't read images from Studio, even with a bridge set |


`rur assets warm` fills the cache ahead of renders: it scans the open place for every image its GUI names
(the `Image`, `HoverImage` and `PressedImage` of ImageLabels and ImageButtons, and ScrollingFrames' bar
images), downloads the ones asset delivery gives and reads the rest from Studio. Afterwards renders find them
cached and need neither Studio nor the network for them. It prints how many were already cached,
downloaded, read from Studio and missing (with each one's reason). `--refresh` reads the images cached from
Studio again. On a live place it found 101 images: a cold warm took 7.3 s, a second 3.5 s.

Studio can open only the images the place's creator owns (others fail with "no permission to load
asset"), so this covers a game's own private art, not another creator's. An id Studio can't read keeps
asset delivery's error, followed by Studio's reason.


ViewportFrames draw their models through a separate 3D worker: [viewports.md](viewports.md).
