import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, renameSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repository = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const archive = resolve(repository, "src-tauri/gen/apple/build/rustle-app_iOS.xcarchive");
const buildNumber = process.argv[2];
if (!buildNumber || !/^[1-9][0-9]*$/.test(buildNumber)) {
  throw new Error("Usage: pnpm build:ios-testflight <positive build number>");
}

function runCommandInRepository(command: string, argumentsToPass: string[]): void {
  const result = spawnSync(command, argumentsToPass, { cwd: repository, stdio: "inherit" });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`${command} failed with exit code ${result.status}`);
}

runCommandInRepository("cargo", ["test", "--workspace", "--lib"]);
runCommandInRepository("cargo", ["check", "-p", "rustle-app"]);
runCommandInRepository("pnpm", ["typecheck"]);
if (existsSync(archive)) {
  const backups = resolve(repository, "target/ios-archive-backups");
  mkdirSync(backups, { recursive: true });
  renameSync(archive, resolve(backups, `${Date.now()}.xcarchive`));
}
runCommandInRepository("xcodegen", ["generate", "--spec", "src-tauri/gen/apple/project.yml"]);
runCommandInRepository("pnpm", [
  "exec", "tauri", "ios", "build", "--target", "aarch64", "--archive-only", "--ci",
  "--config", JSON.stringify({ bundle: { iOS: { bundleVersion: buildNumber } } }),
]);
runCommandInRepository("xcodebuild", [
  "-exportArchive", "-archivePath", archive,
  "-exportOptionsPlist", "src-tauri/gen/apple/ExportOptions.testflight.plist",
  "-exportPath", `target/ios-testflight-${buildNumber}`, "-allowProvisioningUpdates",
]);
console.log(`TestFlight package exported to target/ios-testflight-${buildNumber}. Upload and real-device acceptance remain separate steps.`);
