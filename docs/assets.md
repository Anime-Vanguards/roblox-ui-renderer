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
  download nothing. Use an account made for this. See [SECURITY.md](../SECURITY.md).
- An image asset delivery refuses (private to the game's creator) can be read from an open Roblox Studio
  instead, with no cookie: see [Images from Studio](#images-from-studio).
- `rbxthumb://` thumbnails come from the thumbnail service (its WebP rendition, as Studio draws).

## Images from Studio

With a Studio MCP bridge configured, a render's `rbxassetid://` images that asset delivery refuses are read
from Studio (`cli/studio-images.mjs`): Studio opens each one as an `EditableImage`
(`AssetService:CreateEditableImageAsync`) and returns its pixels, and rur stores them losslessly as
`assets/<id>.png`, recorded with `"source": "studio"`. Later renders use the cache and don't ask Studio
again. `--offline` never asks Studio. The bridge is on 127.0.0.1; the Luau only reads (nothing is mounted,
edited or saved), and no credential is used.

| Variable | Meaning |
|---|---|
| `RUR_STUDIO_MCP_PORT` | the TCP bridge `rur compare` uses ([studio.md](studio.md)) |
| `RUR_STUDIO_MCP_URL` | or an HTTP bridge with `GET /instances` and `POST /mcp/execute_luau` (for example `robloxstudio-mcp --http-only`, `http://127.0.0.1:58741`) |
| `RUR_STUDIO` | the Studio to read from: its id or part of its name (default: each edit session in turn) |
| `RUR_STUDIO_IMAGES=0` | don't read images from Studio, even with a bridge set |


Studio can open only the images the place's creator owns (others fail with "no permission to load
asset"), so this covers a game's own private art, not another creator's. An id Studio can't read keeps
asset delivery's error, followed by Studio's reason.


ViewportFrames draw their models through a separate 3D worker: [viewports.md](viewports.md).
