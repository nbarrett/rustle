import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { createReadStream, existsSync } from "node:fs";
import { mkdtemp, rename } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { setTimeout as sleep } from "node:timers/promises";

const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const installedApp = "/Applications/Rustle.app";
const builtApp = path.join(repositoryRoot, "target/release/bundle/macos/Rustle.app");
const executableInsideApp = path.join("Contents", "MacOS", "rustle-app");

function runInTheRepository(command: string, args: string[]): void {
  const result = spawnSync(command, args, {
    cwd: repositoryRoot,
    stdio: "inherit",
  });
  if (result.status !== 0) {
    throw new Error(`${command} ${args.join(" ")} failed`);
  }
}

function runningRustleProcessIds(): number[] {
  const result = spawnSync(
    "pgrep",
    ["-f", "^/Applications/Rustle.app/Contents/MacOS/rustle-app$"],
    { encoding: "utf8" },
  );
  if (result.status !== 0 && result.status !== 1) {
    throw new Error("Could not inspect running Rustle processes");
  }
  return (result.stdout ?? "")
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line !== "")
    .map((line) => Number(line));
}

async function stopInstalledRustleProcesses(): Promise<void> {
  for (const pid of runningRustleProcessIds()) {
    try {
      process.kill(pid, "SIGTERM");
    } catch {
      continue;
    }
  }
  for (let attempt = 0; attempt < 50; attempt += 1) {
    if (runningRustleProcessIds().length === 0) {
      return;
    }
    await sleep(100);
  }
  throw new Error("Rustle did not stop; the installed app was not replaced");
}

async function sha256OfAppExecutable(app: string): Promise<string> {
  const hash = createHash("sha256");
  for await (const chunk of createReadStream(path.join(app, executableInsideApp))) {
    hash.update(chunk);
  }
  return hash.digest("hex");
}

async function openInstalledRustleAndWaitForTheProcess(): Promise<void> {
  runInTheRepository("open", ["-n", installedApp]);
  for (let attempt = 0; attempt < 50; attempt += 1) {
    if (runningRustleProcessIds().length > 0) {
      return;
    }
    await sleep(100);
  }
  throw new Error("Rustle did not start");
}

async function installTheBuiltAppAndRestart(): Promise<void> {
  runInTheRepository("codesign", ["--verify", "--deep", "--strict", builtApp]);
  const staging = await mkdtemp(path.join(path.dirname(installedApp), ".rustle-update-"));
  const prepared = path.join(staging, "Rustle.app");
  const previous = path.join(staging, "Previous-Rustle.app");
  runInTheRepository("ditto", [builtApp, prepared]);
  runInTheRepository("codesign", ["--verify", "--deep", "--strict", prepared]);
  if ((await sha256OfAppExecutable(prepared)) !== (await sha256OfAppExecutable(builtApp))) {
    throw new Error("The prepared executable does not match the new build");
  }
  await stopInstalledRustleProcesses();
  try {
    await rename(installedApp, previous);
    await rename(prepared, installedApp);
    await openInstalledRustleAndWaitForTheProcess();
    if ((await sha256OfAppExecutable(installedApp)) !== (await sha256OfAppExecutable(builtApp))) {
      throw new Error("The installed executable does not match the new build");
    }
  } catch (error) {
    if (existsSync(previous)) {
      if (existsSync(installedApp)) {
        await rename(installedApp, path.join(staging, "Failed-Rustle.app"));
      }
      await rename(previous, installedApp);
    }
    if (existsSync(installedApp)) {
      await openInstalledRustleAndWaitForTheProcess();
    }
    throw error;
  }
  console.log(`Previous app preserved at ${previous}`);
  console.log(
    "Latest build is installed and running. Real dictation checks remain pending.",
  );
}

async function testBuildInstallAndRestartRustle(): Promise<void> {
  if (process.platform !== "darwin") {
    throw new Error("This build-and-restart command is for macOS");
  }
  runInTheRepository("cargo", ["test", "--workspace", "--lib"]);
  runInTheRepository("pnpm", ["typecheck"]);
  runInTheRepository("pnpm", ["typecheck:web"]);
  runInTheRepository("pnpm", ["exec", "tauri", "build", "--bundles", "app"]);
  await installTheBuiltAppAndRestart();
}

await testBuildInstallAndRestartRustle();
