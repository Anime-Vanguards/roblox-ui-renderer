// Studio asset snapshot: exports Studio-only instances the stories reach (GUI templates kept in
// ServerStorage and similar, which offline runs only know as placeholders with default properties)
// from the open Studio into `<Service>.rbxm` files the runtime loads with --snapshot-dir. `rur compare`
// passes .cache/snapshots/<studio>/ to its offline run when that directory exists.
//
//   node tools/sweep/studio-assets.mjs [--studio <title part>] [--out <dir>] [--from-sweep [<sweep stories dir>]] [paths...]
//   node tools/sweep/studio-assets.mjs --all-gui [--studio <title part>] [--out <dir>]
//
// Paths are full names ("ServerStorage.Templates.Alert"); --from-sweep collects every
// `diagnostics.studioOnly.gui` entry of a sweep's scenes (default out/sweep/stories). --all-gui exports
// every outermost GUI and UI component (GuiBase2d / UIBase) under the services scripts read templates
// from, replacing the whole snapshot directory (--out, else <state dir>/snapshots/<studio>)
// and writing snapshot.json beside it (Studio, place, time, counts). Studio is only read
// (SerializationService:SerializeInstancesAsync), under the Studio lock.

import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { OUT_DIR, stateDir } from "../../cli/paths.mjs";

import { runLuau, luauString, DEFAULT_STUDIO } from "../../cli/studio.mjs";
import { withStudioLock } from "../../cli/lock.mjs";
import os from "node:os";

// Lune (rokit, rokit.toml) runs assemble-snapshot.luau: its @lune/roblox writes the .rbxm files
function findLune() {
	if (process.env.RUR_LUNE && fs.existsSync(process.env.RUR_LUNE)) return process.env.RUR_LUNE;
	const installed = path.join(os.homedir(), ".rokit", "tool-storage", "lune-org", "lune", "0.10.4", process.platform === "win32" ? "lune.exe" : "lune");
	return fs.existsSync(installed) ? installed : "lune";
}

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "..");

const args = { paths: [] };
for (let i = 2; i < process.argv.length; i++) {
	const arg = process.argv[i];
	if (arg === "--studio" || arg === "--out" || arg === "--place") {
		args[arg.slice(2)] = process.argv[++i];
	} else if (arg === "--all-gui") {
		args.allGui = true;
	} else if (arg === "--from-sweep") {
		const next = process.argv[i + 1];
		args.fromSweep = next && !next.startsWith("--") && fs.existsSync(next) ? process.argv[++i] : path.join(OUT_DIR, "sweep", "stories");
	} else {
		args.paths.push(arg);
	}
}
if (args.place) {
	process.stderr.write("studio-assets: --place: no place has an implicit snapshot; name the Studio (--studio) and the directory (--out)\n");
	process.exit(2);
}
// The Studio is found by a part of its title (cli/studio.mjs: RUR_STUDIO, else the only one open); the
// snapshot is named after it
const studio = args.studio || DEFAULT_STUDIO;
const outDir = path.resolve(args.out || path.join(stateDir(), "snapshots", studio || "default"));
// --all-gui: the services whose GUI templates scripts clone (Workspace and the 3D world are not exported)
const GUI_SERVICES = ["ReplicatedFirst", "ReplicatedStorage", "ServerScriptService", "ServerStorage", "StarterGui", "StarterPack", "StarterPlayer"];

// Placeholder names as the runtime reports them: "<full name> (<class>)"
function fromSweep(dir) {
	const found = new Set();
	for (const name of fs.readdirSync(dir)) {
		const file = path.join(dir, name, "scene.json");
		if (!fs.existsSync(file)) {
			continue;
		}
		let scene;
		try {
			scene = JSON.parse(fs.readFileSync(file, "utf8"));
		} catch {
			continue;
		}
		const gui = scene.diagnostics && scene.diagnostics.studioOnly && scene.diagnostics.studioOnly.gui;
		for (const entry of Array.isArray(gui) ? gui : []) {
			found.add(String(entry).replace(/ \([A-Za-z0-9]+\)$/, ""));
		}
	}
	return [...found];
}

const names = [...args.paths, ...(args.fromSweep ? fromSweep(args.fromSweep) : [])];
if (!names.length && !args.allGui) {
	process.stderr.write("studio-assets: no instance paths (give full names, --from-sweep or --all-gui)\n");
	process.exit(2);
}

// Nested templates are covered by their outermost exported ancestor
const sorted = [...new Set(names)].sort((a, b) => a.length - b.length);
const wanted = sorted.filter((name, index) => !sorted.slice(0, index).some(outer => name.startsWith(outer + ".")));
const pathParts = wanted.map(name => name.split("."));

const LUAU = `
local HttpService = game:GetService("HttpService")
local AllGuiServices = HttpService:JSONDecode(${luauString(JSON.stringify(args.allGui ? GUI_SERVICES : []))})
local SerializationService = game:GetService("SerializationService")
local Alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"

local function Base64(Data)
	local Length = buffer.len(Data)
	local Out = buffer.create(math.ceil(Length / 3) * 4)
	local Write = 0
	for Index = 0, Length - 1, 3 do
		local A = buffer.readu8(Data, Index)
		local B = if Index + 1 < Length then buffer.readu8(Data, Index + 1) else 0
		local C = if Index + 2 < Length then buffer.readu8(Data, Index + 2) else 0
		local N = A * 65536 + B * 256 + C
		buffer.writeu8(Out, Write, string.byte(Alphabet, bit32.rshift(N, 18) + 1))
		buffer.writeu8(Out, Write + 1, string.byte(Alphabet, bit32.band(bit32.rshift(N, 12), 63) + 1))
		buffer.writeu8(Out, Write + 2, if Index + 1 < Length then string.byte(Alphabet, bit32.band(bit32.rshift(N, 6), 63) + 1) else 61)
		buffer.writeu8(Out, Write + 3, if Index + 2 < Length then string.byte(Alphabet, bit32.band(N, 63) + 1) else 61)
		Write += 4
	end
	return buffer.tostring(Out)
end

-- Targets: { Path, Instance, Chain } from the given paths, else every outermost GUI object or UI component
local Targets = {}
for _, Path in HttpService:JSONDecode(${luauString(JSON.stringify(pathParts))}) do
	local Ok, Service = pcall(function()
		return game:GetService(Path[1])
	end)
	local Current = if Ok then Service else nil
	local Chain = {}
	for Index = 2, #Path do
		if not Current then
			break
		end
		if Index > 2 then
			table.insert(Chain, { name = Current.Name, className = Current.ClassName })
		end
		Current = Current:FindFirstChild(Path[Index])
	end
	table.insert(Targets, { Path = Path, Instance = if Current ~= Service then Current else nil, Chain = Chain })
end

local function IsGui(Instance)
	return Instance:IsA("GuiBase2d") or Instance:IsA("UIBase")
end

local function Collect(Service, Parent, Chain)
	for _, Child in Parent:GetChildren() do
		if IsGui(Child) then
			local Path = { Service.Name }
			for _, Link in Chain do
				table.insert(Path, Link.name)
			end
			table.insert(Path, Child.Name)
			table.insert(Targets, { Path = Path, Instance = Child, Chain = table.clone(Chain) })
		else
			table.insert(Chain, { name = Child.Name, className = Child.ClassName })
			Collect(Service, Child, Chain)
			table.remove(Chain)
		end
	end
end

for _, Name in AllGuiServices do
	local Ok, Service = pcall(function()
		return game:GetService(Name)
	end)
	if Ok and Service then
		Collect(Service, Service, {})
	end
end

local Results = {}
for _, Target in Targets do
	local Path, Current, Chain = Target.Path, Target.Instance, Target.Chain
	local Entry = { path = Path }
	if not Current then
		Entry.error = "not found in Studio"
	else
		-- An unparented copy without tags: Lune's rbx-dom (0.10.4) rejects the Tags property Studio writes
		-- ("ImageLabel.Tags should be SharedString, but it was Tags")
		local Serialized, Data = pcall(function()
			local Copy = Current:Clone()
			assert(Copy, "not archivable")
			local Nodes = Copy:GetDescendants()
			table.insert(Nodes, Copy)
			for _, Node in Nodes do
				for _, Tag in Node:GetTags() do
					Node:RemoveTag(Tag)
				end
			end
			local Result = SerializationService:SerializeInstancesAsync({ Copy })
			Copy:Destroy()
			return Result
		end)
		if Serialized then
			Entry.service = Path[1]
			Entry.chain = Chain
			Entry.className = Current.ClassName
			Entry.descendants = #Current:GetDescendants()
			Entry.data = Base64(Data)
		else
			Entry.error = tostring(Data)
		end
	end
	table.insert(Results, Entry)
end
return HttpService:JSONEncode(Results)
`;

// Lune 0.10.4's rbx-dom does not know the property types newer Studios write for these (it rejects the
// whole file: "ImageLabel.Tags should be SharedString, but it was Tags"); their PROP chunks are dropped
const UNREADABLE_PROPERTIES = new Set(["Tags"]);

// LZ4 block decompression (binary model chunks)
function lz4(source, length) {
	const out = Buffer.alloc(length);
	let read = 0;
	let write = 0;
	while (read < source.length) {
		const token = source[read++];
		let literals = token >> 4;
		if (literals === 15) {
			let byte;
			do {
				byte = source[read++];
				literals += byte;
			} while (byte === 255);
		}
		source.copy(out, write, read, read + literals);
		read += literals;
		write += literals;
		if (read >= source.length) {
			break;
		}

		const offset = source[read] | (source[read + 1] << 8);
		read += 2;
		let match = token & 15;
		if (match === 15) {
			let byte;
			do {
				byte = source[read++];
				match += byte;
			} while (byte === 255);
		}
		match += 4;
		for (let index = 0; index < match; index++) {
			out[write] = out[write - offset];
			write++;
		}
	}
	return out;
}

// A binary model without the PROP chunks of UNREADABLE_PROPERTIES (other chunks are copied as they are)
function stripProperties(model) {
	const parts = [model.subarray(0, 32)];
	let offset = 32;
	while (offset < model.length) {
		const name = model.subarray(offset, offset + 4).toString("latin1");
		const compressed = model.readUInt32LE(offset + 4);
		const length = model.readUInt32LE(offset + 8);
		const size = 16 + (compressed || length);
		const chunk = model.subarray(offset, offset + size);
		offset += size;
		if (name === "PROP") {
			const payload = model.subarray(offset - size + 16, offset);
			if (compressed && payload.readUInt32BE(0) === 0x28b52ffd) {
				throw new Error("zstd-compressed model chunks are not supported");
			}
			const data = compressed ? lz4(payload, length) : payload;
			const nameLength = data.readUInt32LE(4);
			if (UNREADABLE_PROPERTIES.has(data.subarray(8, 8 + nameLength).toString("utf8"))) {
				continue;
			}
		}
		parts.push(chunk);
		if (name === "END\u0000") {
			break;
		}
	}
	return Buffer.concat(parts);
}

const started = Date.now();
let exported;
try {
	exported = await withStudioLock("studio-assets", async () => {
		const answer = await runLuau(LUAU, { studio });
		return { results: JSON.parse(answer.text), studioName: answer.studio && answer.studio.name };
	});
} catch (error) {
	process.stderr.write(`studio-assets: ${String((error && error.message) || error).split("\n")[0]}\n`);
	process.exit(1);
}
const { results, studioName } = exported;

// --all-gui replaces the whole snapshot: it is built beside it and swapped in, so a render never reads half of one
const buildDir = args.allGui ? `${outDir}.new` : outDir;
fs.rmSync(`${outDir}.new`, { recursive: true, force: true });
const partsDir = path.join(buildDir, ".parts");
fs.mkdirSync(partsDir, { recursive: true });
const items = [];
let descendants = 0;
let skipped = 0;
for (const [index, result] of results.entries()) {
	const name = result.path.join(".");
	if (result.error) {
		skipped++;
		process.stdout.write(`skipped ${name}: ${result.error}\n`);
		continue;
	}
	const file = path.join(partsDir, `${index}.rbxm`);
	fs.writeFileSync(file, stripProperties(Buffer.from(result.data, "base64")));
	items.push({ service: result.service, chain: result.chain, file });
	descendants += result.descendants + 1;
	if (!args.allGui) {
		process.stdout.write(`exported ${name} (${result.className}, ${result.descendants} descendants)\n`);
	}
}
if (!items.length) {
	process.exit(1);
}

const specFile = path.join(partsDir, "spec.json");
fs.writeFileSync(specFile, JSON.stringify({ out: buildDir, items }, null, 1));
const assembled = spawnSync(findLune(), ["run", path.join(ROOT, "tools", "sweep", "assemble-snapshot.luau"), specFile], { cwd: ROOT, encoding: "utf8" });
if (!args.allGui) {
	process.stdout.write(assembled.stdout);
}
if (assembled.status !== 0) {
	process.stderr.write(assembled.stderr);
	process.exit(1);
}

if (args.allGui) {
	fs.rmSync(partsDir, { recursive: true, force: true });
	const manifest = { studio: studioName || studio, place: args.place || null, exportedAt: new Date().toISOString(), templates: items.length, instances: descendants, skipped, services: GUI_SERVICES };
	fs.writeFileSync(path.join(buildDir, "snapshot.json"), JSON.stringify(manifest, null, 1));
	const previous = `${outDir}.old`;
	fs.rmSync(previous, { recursive: true, force: true });
	if (fs.existsSync(outDir)) {
		fs.renameSync(outDir, previous);
	}
	fs.renameSync(buildDir, outDir);
	fs.rmSync(previous, { recursive: true, force: true });
	process.stdout.write(`snapshot ${outDir}: ${items.length} templates (${descendants} instances) from ${manifest.studio}${skipped ? `, ${skipped} skipped` : ""} in ${((Date.now() - started) / 1000).toFixed(1)} s\n`);
}
