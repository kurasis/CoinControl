// @vitest-environment node
import { expect, it } from "vitest";
import {
  mkdtempSync,
  mkdirSync,
  readFileSync,
  writeFileSync,
  cpSync,
  readdirSync,
  rmSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawnSync } from "node:child_process";

it.each(["cargo", "prettier"])(
  "preserves existing bindings and removes staging output when %s fails",
  (failed) => {
    const fixture = mkdtempSync(join(tmpdir(), "coincontrol bindings test-"));
    try {
      mkdirSync(join(fixture, "scripts"));
      for (const name of ["gen-bindings.mjs", "lib.mjs"])
        cpSync(new URL(name, import.meta.url), join(fixture, "scripts", name));
      const bindings = join(fixture, "src", "ipc", "bindings");
      mkdirSync(bindings, { recursive: true });
      writeFileSync(join(bindings, "Existing.ts"), "// original binding\n");
      const bin = join(fixture, "bin");
      mkdirSync(bin);
      const windows = process.platform === "win32";
      const code = failed === "cargo" ? 42 : 0;
      writeFileSync(
        join(bin, windows ? "cargo.cmd" : "cargo"),
        windows ? `@echo off\r\nexit /b ${code}\r\n` : `#!/bin/sh\nexit ${code}\n`,
        { mode: 0o755 },
      );
      const formatter = join(fixture, "node_modules", "prettier", "bin");
      mkdirSync(formatter, { recursive: true });
      writeFileSync(join(formatter, "prettier.cjs"), "process.exit(42);\n");
      const result = spawnSync(process.execPath, ["scripts/gen-bindings.mjs"], {
        cwd: fixture,
        env: { ...process.env, PATH: `${bin}${windows ? ";" : ":"}${process.env.PATH}` },
        encoding: "utf8",
      });
      expect(result.status).toBe(42);
      expect(readFileSync(join(bindings, "Existing.ts"), "utf8")).toBe("// original binding\n");
      expect(readdirSync(bindings)).toEqual(["Existing.ts"]);
      expect(readdirSync(join(fixture, "target"))).toEqual([]);
    } finally {
      rmSync(fixture, { recursive: true, force: true });
    }
  },
);
