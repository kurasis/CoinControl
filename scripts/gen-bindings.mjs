// Regenerates src/ipc/bindings/*.ts from the Rust DTOs (ts-rs).
import { rmSync } from "node:fs";
import { run } from "./lib.mjs";

rmSync("src/ipc/bindings", { recursive: true, force: true });
run("cargo", [
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
]);
run("npx", ["prettier", "--log-level", "warn", "--write", "src/ipc/bindings"]);
