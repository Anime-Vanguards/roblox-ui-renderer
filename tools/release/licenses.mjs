// THIRD_PARTY_LICENSES.txt for one target: the license texts of exactly the crates compiled into
// the binary, plus the license files of the C/C++ code and fonts that come with them.
//
//   node tools/release/licenses.mjs --target <triple> --freetype <source dir> --out <file> [--metadata <json>]
//
// The crates are the target's normal dependencies (not dev or build dependencies) with the default
// features, from `cargo metadata`. Nothing is downloaded and no package is needed.
import fs from "node:fs";
import path from "node:path";
import { execFileSync } from "node:child_process";
import { ROOT, isMain, options } from "./version.mjs";

// File names that hold a license or notice
const LEGAL_NAME = /^(LICENSE|LICENCE|COPYING|NOTICE|UNLICENSE|COPYRIGHT)/i;

const read = file => fs.readFileSync(file, "utf8");
const slashes = file => file.split(path.sep).join("/");

// Orders strings by UTF-16 code unit. `localeCompare` depends on the machine's locale, and the
// generated file must be the same on every runner.
const compare = (a, b) => {
	if (a < b) return -1;
	return a > b ? 1 : 0;
};

// A folder's entries by name. The file system's own order differs between platforms, and the order
// of the license files in the output must not.
const sortedEntries = directory => fs.readdirSync(directory, { withFileTypes: true }).sort((a, b) => compare(a.name, b.name));

// The license files in a package root, and every file in its LICENSES folder
export function legalFiles(directory, insideLicensesFolder = false) {
	if (!fs.existsSync(directory)) return [];
	const result = [];
	for (const entry of sortedEntries(directory)) {
		const full = path.join(directory, entry.name);
		if (entry.isFile() && (insideLicensesFolder || LEGAL_NAME.test(entry.name))) result.push(full);
		else if (entry.isDirectory() && (insideLicensesFolder || entry.name.toUpperCase() === "LICENSES")) result.push(...legalFiles(full, true));
	}
	return result;
}

// The optional dependencies that the features enabled for `pkg` switch on. The resolve node lists
// the enabled features; each one expands into its members: other features, `dep:name`, `name/feature`
// and the weak `name?/feature`.
function enabledOptionalDependencies(pkg, node) {
	const enabled = new Set();
	const expanded = new Set();
	const expand = feature => {
		if (expanded.has(feature)) return; // features can name each other in a cycle
		expanded.add(feature);
		if (feature.startsWith("dep:")) {
			enabled.add(feature.slice("dep:".length));
		} else if (feature.includes("/")) {
			// `name/feature` enables the dependency. The weak form `name?/feature` only configures it
			// when something else enables it.
			const [dependency] = feature.split("/");
			if (!dependency.endsWith("?")) enabled.add(dependency);
		} else {
			for (const member of pkg.features?.[feature] ?? []) expand(member);
		}
	};
	for (const feature of node.features ?? []) expand(feature);
	return enabled;
}

// The manifest entries behind one kind of a resolved dependency edge of `pkg`: the normal
// declarations for the same target whose package or rename the edge names. The resolve graph writes a
// renamed dependency's hyphens as underscores.
function manifestDeclarations(pkg, edge, edgeKind, packages) {
	return (pkg.dependencies ?? []).filter(declared => {
		if (declared.kind !== null || declared.target !== edgeKind.target) return false;
		if (declared.rename) return declared.rename.replaceAll("-", "_") === edge.name;
		return declared.name === packages.get(edge.pkg).name;
	});
}

// Is the package at the end of this resolve edge linked into the binary? The graph is already filtered
// by target but still lists optional dependencies that no enabled feature switches on.
function isLinked(pkg, edge, enabledOptional, packages) {
	return edge.dep_kinds.some(edgeKind => {
		if (edgeKind.kind !== null) return false; // dev and build dependencies are not linked
		return manifestDeclarations(pkg, edge, edgeKind, packages)
			.some(declared => !declared.optional || enabledOptional.has(declared.rename ?? declared.name));
	});
}

// The packages the binary links: the binary's own package and everything reachable from it through
// normal dependencies, sorted by name and version
export function normalPackages(metadata, name = "rur-native") {
	const root = metadata.packages.find(pkg => pkg.name === name && metadata.workspace_members.includes(pkg.id));
	if (!root) throw new Error(`the metadata has no package ${name} in the workspace`);

	const nodes = new Map(metadata.resolve.nodes.map(node => [node.id, node]));
	const packages = new Map(metadata.packages.map(pkg => [pkg.id, pkg]));
	const reached = new Set();
	const visit = id => {
		if (reached.has(id)) return;
		reached.add(id);
		const node = nodes.get(id);
		if (!node) throw new Error(`the metadata has no resolve node for ${id}`);
		const pkg = packages.get(id);
		const enabledOptional = enabledOptionalDependencies(pkg, node);
		for (const edge of node.deps) {
			if (isLinked(pkg, edge, enabledOptional, packages)) visit(edge.pkg);
		}
	};
	visit(root.id);

	return metadata.packages
		.filter(pkg => reached.has(pkg.id))
		.sort((a, b) => compare(a.name, b.name) || compare(a.version, b.version));
}

// Every license file in a package: the root's (see legalFiles), then, below the root, embedded
// fonts and vendored C/C++ code, which keep their own notices (OFL.txt, UFL.txt, anything named
// like a license). Build output, tests and examples are skipped.
function vendoredFiles(directory) {
	const result = legalFiles(directory);
	for (const entry of sortedEntries(directory)) {
		const full = path.join(directory, entry.name);
		const isLicenseFile = /^(OFL|UFL)\.txt$/i.test(entry.name) || /licen[cs]e/i.test(entry.name);
		if (entry.isFile() && isLicenseFile && !result.includes(full)) result.push(full);
		if (entry.isDirectory() && ![".git", "target", "tests", "examples", "LICENSES"].includes(entry.name)) result.push(...vendoredFiles(full));
	}
	return result;
}

// Text that identifies a file as the standard text of a license
const STANDARD_TEXT_MARKERS = new Map([
	["MIT", body => body.includes("Permission is hereby granted, free of charge")],
	["Apache-2.0", body => body.includes("Apache License")],
	["MPL-2.0", body => body.includes("Mozilla Public License")],
	["OFL-1.1", body => /SIL OPEN FONT LICENSE/i.test(body)],
]);

// Is `file` of `pkg` the standard text of the license `token` (an SPDX id)?
function isStandardText(token, pkg, file, body) {
	const name = path.basename(file).toLowerCase();
	const family = token.toLowerCase().replace(/-\d.*$/, ""); // "apache-2.0" -> "apache"
	const isCc0 = token === "CC0-1.0" && name.includes("cc0") && /CC0|Creative Commons/i.test(body);
	// The package's own license file, when the package declares exactly this license
	const isOwnFile = pkg.license === token && LEGAL_NAME.test(name) && !/^notice|^copyright/i.test(name);
	const hasMarker = name.includes(family) && Boolean(STANDARD_TEXT_MARKERS.get(token)?.(body));
	const isUbuntuFont = token === "Ubuntu-font-1.0" && name === "ufl.txt" && /UBUNTU FONT LICEN[CS]E/i.test(body);
	return isCc0 || isOwnFile || hasMarker || isUbuntuFont;
}

// The standard text of a license, taken from any package of the graph that ships it, or undefined
// when none does. A package with no license file of its own is covered by this. A file that cannot
// be read stops the run, so an unreadable text never makes a different license the one used.
function findStandardText(metadata, token) {
	for (const pkg of metadata.packages) {
		for (const file of vendoredFiles(path.dirname(pkg.manifest_path))) {
			const body = read(file);
			if (isStandardText(token, pkg, file, body)) return body;
		}
	}
	return undefined;
}

// A package with no license file is listed with its declared license, and the standard text of
// that license goes once into the appendix. "A AND B" needs both texts; for "A OR B" the first one
// that can be found is enough.
function addStandardTexts(pkg, metadata, appendix) {
	if (!pkg.license) throw new Error(`${pkg.name}: no declared license or license file`);
	const clauses = pkg.license.replace(/[()]/g, "").replace(/\s*\/\s*/g, " OR ").split(/\s+AND\s+/);
	for (const clause of clauses) {
		let found = false;
		for (const token of clause.split(/\s+OR\s+/)) {
			const body = findStandardText(metadata, token);
			if (body === undefined) continue;
			appendix.set(token, body);
			found = true;
			break;
		}
		if (!found) throw new Error(`${pkg.name}: no license text for ${clause}`);
	}
}

function describeSource(pkg, root, directory) {
	if (pkg.source?.startsWith("registry+")) return `crates.io (${pkg.source})`;
	return pkg.source || `path: ${slashes(path.relative(root, directory)) || "."}`;
}

// One file's text under a "--- name ---" line
const fileSection = (name, file) => `\n--- ${name} ---\n${read(file)}\n`;

// FreeType's license files, from the source tree the binary was built with
const FREETYPE_LICENSE_FILES = ["LICENSE.TXT", "docs/GPLv2.TXT", "docs/FTL.TXT", "src/bdf/README", "src/pcf/README", "src/gzip/zlib.h"];

export function licenses(metadata, { root = ROOT, freetype } = {}) {
	const packages = normalPackages(metadata);
	const appendix = new Map(); // license id -> standard text
	const sections = ["THIRD-PARTY LICENSES\nTarget-filtered normal dependency graph; public default features.\n"];

	for (const pkg of packages) {
		const directory = path.dirname(pkg.manifest_path);
		const files = legalFiles(directory);
		const source = describeSource(pkg, root, directory);
		sections.push(`\n${"=".repeat(72)}\n${pkg.name} ${pkg.version}\nSource: ${source}\nLicense: ${pkg.license || "not declared"}\n`);
		for (const file of files) sections.push(fileSection(slashes(path.relative(directory, file)), file));
		if (!files.length) addStandardTexts(pkg, metadata, appendix);
		// Embedded fonts and vendored C/C++ keep additional notices below the package root
		for (const file of vendoredFiles(directory).filter(file => !files.includes(file))) {
			sections.push(fileSection(slashes(path.relative(directory, file)), file));
		}
	}

	// The VM's source crate is only a build dependency of the engine, but its C++ is linked into the binary
	const engine = packages.find(pkg => pkg.name === "roblox-engine-emulator");
	if (engine) {
		const engineDirectory = path.dirname(engine.manifest_path);
		const files = vendoredFiles(path.join(engineDirectory, "vendor/luau0-src"));
		if (!files.length) throw new Error("the engine's vendored Luau has no license files");
		for (const file of files) sections.push(fileSection(`engine/${slashes(path.relative(engineDirectory, file))}`, file));
	}

	if (freetype) {
		for (const name of FREETYPE_LICENSE_FILES) sections.push(fileSection(`FreeType 2.14.2/${name}`, path.join(freetype, name)));
	}

	for (const [name, body] of [...appendix].sort(([a], [b]) => compare(a, b))) {
		sections.push(`\n--- Standard license appendix: ${name} ---\n${body}\n`);
	}
	return sections.join("");
}

// `cargo metadata` for the binary's workspace, limited to the packages the target builds
function cargoMetadata(target) {
	const command = ["metadata", "--locked", "--format-version", "1", "--filter-platform", target];
	return execFileSync("cargo", command, { cwd: path.join(ROOT, "native"), encoding: "utf8", maxBuffer: 64 * 1024 * 1024 });
}

if (isMain(import.meta.url)) {
	const args = options(process.argv.slice(2));
	if (!args.target || !args.out || !args.freetype) throw new Error("usage: licenses.mjs --target <triple> --out <file> --freetype <source> [--metadata <json>]");
	// --metadata reuses a saved `cargo metadata` output instead of running cargo
	const metadata = JSON.parse(args.metadata ? read(args.metadata) : cargoMetadata(args.target));
	fs.mkdirSync(path.dirname(path.resolve(args.out)), { recursive: true });
	fs.writeFileSync(args.out, licenses(metadata, { freetype: args.freetype }));
}
