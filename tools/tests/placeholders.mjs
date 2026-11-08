// `rur render --placeholders`: an image the render cannot get is drawn as a labelled checkerboard
// (native/src/placeholder.rs) written into the render's own assets/, and is still listed in missingImages;
// without the flag nothing is drawn there, as in Studio. Rendered offline (the id is never cached), in a
// process of its own and through the render service.
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { PNG } from "pngjs";

const root = fileURLToPath(new URL("../../", import.meta.url));
const dir = fs.mkdtempSync(path.join(os.tmpdir(), "rur-placeholders-"));
const fixture = path.join(dir, "missing.luau");
fs.writeFileSync(fixture, `return function(Target)
	local Icon = Instance.new("ImageLabel")
	Icon.BackgroundTransparency = 1
	Icon.Position = UDim2.fromOffset(20, 20)
	Icon.Size = UDim2.fromOffset(64, 64)
	Icon.Image = "rbxassetid://424242"
	Icon.Parent = Target
end
`);

function render(name, { placeholders, service }) {
	const out = path.join(dir, name);
	const env = { ...process.env, RUR_STATE_DIR: path.join(dir, "state"), RUR_NO_SERVICE: service ? "" : "1", RUR_ISOLATED: "" };
	const args = ["cli/rur.mjs", "render", "--fixture", fixture, "--roots", path.join(root, "tools/tests/oracle-root"), "--viewport", "200x120", "--background", "40,44,60", "--no-snapshots", "--offline", "--out", out, "--json", ...(placeholders ? ["--placeholders"] : [])];
	const result = spawnSync(process.execPath, args, { cwd: root, env, encoding: "utf8", windowsHide: true });
	assert.equal(result.status, 0, result.stderr);
	const summary = JSON.parse(result.stdout.trim().split("\n").pop());
	const manifest = JSON.parse(fs.readFileSync(path.join(out, "manifest.json"), "utf8"));
	const image = PNG.sync.read(fs.readFileSync(path.join(out, "render.png")));
	// The icon's square (physical pixels at scale 1.5): how many are the checkerboard's magenta
	let magenta = 0;
	for (let y = 32; y < 124; y++) {
		for (let x = 32; x < 124; x++) {
			const at = (y * image.width + x) * 4;
			if (image.data[at] > 200 && image.data[at + 1] < 60 && image.data[at + 2] > 200) magenta++;
		}
	}
	return { summary, entry: manifest.images["rbxassetid://424242"], out, magenta };
}

try {
	for (const service of [false, true]) {
		const plain = render(`plain-${service}`, { placeholders: false, service });
		assert.equal(plain.entry.src, undefined);
		assert.equal(plain.magenta, 0, "without --placeholders a missing image draws nothing");

		const shown = render(`shown-${service}`, { placeholders: true, service });
		assert.equal(shown.entry.src, "assets/missing-424242.png");
		assert.equal(shown.entry.placeholder, true);
		assert.match(shown.entry.error, /offline and not cached/);
		assert.ok(fs.existsSync(path.join(shown.out, "assets", "missing-424242.png")));
		assert.ok(!fs.existsSync(path.join(dir, "state", "assets", "missing-424242.png")), "never written into the asset cache");
		assert.ok(shown.summary.missingImages.some(line => line.startsWith("rbxassetid://424242:")), "still reported missing");
		assert.ok(shown.magenta > 92 * 92 * 0.3, `the checkerboard is drawn (${shown.magenta} magenta pixels)`);
	}
	console.log("Placeholders: missing images drawn as a labelled checkerboard with --placeholders (in process and in the service), still reported, nothing drawn without it.");
} finally {
	const client = await import("../../cli/client.mjs");
	await client.stop({ env: { ...process.env, RUR_STATE_DIR: path.join(dir, "state") } });
	// (the service lets go of its log as it exits)
	fs.rmSync(dir, { recursive: true, force: true, maxRetries: 20, retryDelay: 100 });
}
