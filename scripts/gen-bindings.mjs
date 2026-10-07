// Regenerates src/ipc/bindings/*.ts from the Rust DTOs (ts-rs).
import { existsSync, mkdirSync, mkdtempSync, renameSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { run } from "./lib.mjs";

// Keep the last usable bindings until both generation and formatting succeed.
mkdirSync("target", { recursive: true });
const staging = mkdtempSync(resolve("target", "bindings-"));
const generated = join(staging, "next");
mkdirSync(generated);
// The staging directory is under ignored build output; format it explicitly.
const ignorePath = join(staging, ".prettierignore");
writeFileSync(ignorePath, "");
// run() exits on child failure, so cleanup also runs on that path.
process.once("exit", () => rmSync(staging, { recursive: true, force: true }));
run(
  "cargo",
  [
    "test",
    "-p",
    "portfolio-core",
    "-p",
    "portfolio-store",
    "-p",
    "portfolio-providers",
    "-p",
    "portfolio-desk",
    "--features",
    "portfolio-core/ts,portfolio-store/ts,portfolio-providers/ts,portfolio-desk/ts",
    "--lib",
    "export_bindings",
    "--quiet",
  ],
  { env: { ...process.env, TS_RS_EXPORT_DIR: generated } },
);
// Invoke the installed CLI without a shell so paths with spaces work on Windows.
run(
  process.execPath,
  [
    resolve("node_modules/prettier/bin/prettier.cjs"),
    "--ignore-path",
    ignorePath,
    "--log-level",
    "warn",
    "--write",
    generated,
  ],
  { shell: false },
);

const destination = resolve("src/ipc/bindings");
const previous = join(staging, "previous");
const hadBindings = existsSync(destination);
if (hadBindings) renameSync(destination, previous);
try {
  renameSync(generated, destination);
} catch (error) {
  if (hadBindings) renameSync(previous, destination);
  throw error;
}
