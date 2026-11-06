// A project's places (docs/projects.md): named sets of source roots, read from the project's rur.json,
// for runs that give no --roots (rur render / stories / sweep, the tools that mount a project).
//
//   { "places": [ { "name": "client", "roots": ["shared", "client"] }, { "name": "server", "roots": ["shared", "server"] } ] }
//
// Roots are relative to the project directory (absolute ones are kept as they are). The engine reads its
// per-root settings from the same rur.json (placeIds, modelAssets, ...) keyed by the root folder's name, so
// roots are usually direct children of the project. The first place is the default. A project without
// `places` (or without a rur.json) has one place, "default", whose root is the project directory itself.
// native/src/places.rs reads the same file the same way.

import fs from "node:fs";
import path from "node:path";

export const DEFAULT_PLACE = "default";

// [{ name, roots: [absolute dirs] }] in the order rur.json lists them; never empty
export function readPlaces(project) {
	const dir = path.resolve(String(project));
	let declared = [];
	try {
		const config = JSON.parse(fs.readFileSync(path.join(dir, "rur.json"), "utf8").replace(/^﻿/, ""));
		if (Array.isArray(config?.places)) declared = config.places;
	} catch {
		// no rur.json, or not JSON: the default place
	}
	const places = [];
	for (const place of declared) {
		const name = place && typeof place.name === "string" ? place.name : "";
		if (!name || places.some(known => known.name === name)) continue;
		const roots = Array.isArray(place.roots) ? place.roots.filter(root => typeof root === "string" && root).map(root => path.resolve(dir, root)) : [];
		places.push({ name, roots });
	}
	if (!places.length) places.push({ name: DEFAULT_PLACE, roots: [dir] });
	return places;
}

export const defaultPlace = project => readPlaces(project)[0].name;

// The roots of the named place (the default place when `place` is not given); [] for a place the project
// does not have
export function placeRoots(project, place) {
	const places = readPlaces(project);
	const found = place ? places.find(item => item.name === place) : places[0];
	return found ? [...found.roots] : [];
}

const inside = (root, file) => {
	const relative = path.relative(root, file);
	return relative === "" || (!relative.startsWith("..") && !path.isAbsolute(relative));
};

// The place a file runs in: the first place with a root that holds it, else the default place
export function placeOf(project, file) {
	const places = readPlaces(project);
	const resolved = path.resolve(String(file));
	return (places.find(place => place.roots.some(root => inside(root, resolved))) || places[0]).name;
}

// Every root of the project once, with the first place that lists it: [{ root, place }]
export function projectRootList(project) {
	const result = [];
	for (const place of readPlaces(project)) {
		for (const root of place.roots) {
			if (!result.some(item => item.root === root)) result.push({ root, place: place.name });
		}
	}
	return result;
}
