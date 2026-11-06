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
- `rbxthumb://` thumbnails come from the thumbnail service (its WebP rendition, as Studio draws).


ViewportFrames draw their models through a separate 3D worker: [viewports.md](viewports.md).
