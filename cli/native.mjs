#!/usr/bin/env node
// Launches the native app (`rur native`): the desktop storybook, or headless captures, benchmarks and
// batches with --headless / --benchmark / --batch (native/src/lib.rs NativeOptions), building it first
// when the sources changed.
import { spawnSync } from "node:child_process";
import path from "node:path";
import { buildNativeApp } from "../tools/native-app/build.mjs";
import { ROOT, describeNativeApp, nativeAppStatus } from "../tools/native-app/config.mjs";

const args = process.argv.slice(2);
if (args[0] === "--") args.shift();
const hasOption = name => args.some(arg => arg === name || arg.startsWith(`${name}=`));
if (!hasOption("--project") && process.env.RUR_PROJECT) args.push("--project", path.resolve(process.env.RUR_PROJECT));
const status = nativeAppStatus({ profile: hasOption("--benchmark") ? "release" : undefined });
if (status.state !== "current") console.error(`rur: ${describeNativeApp(status)}; building`);
const executable = status.state === "current" ? status.executable : await buildNativeApp();
const launchArgs = hasOption("--repo") ? args : ["--repo", ROOT, ...args];
const result = spawnSync(executable, launchArgs, { stdio: "inherit", windowsHide: true, env: process.env });
if (result.error) throw result.error;
process.exitCode = result.status ?? 1;
