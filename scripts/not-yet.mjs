// Placeholder for required commands whose implementation belongs to a later
// delivery stage. It fails loudly so nothing is mistaken for a passing check.
const command = process.argv[2] ?? "this command";
const notes = {
  "test:e2e:windows": "Native Windows end-to-end tests (TESTING.md Layer C) arrive in stage E.",
  "verify:release": "Release artifact inspection (TESTING.md §6) arrives in stage E.",
};
console.error(`BLOCKED: ${command} is not implemented yet. ${notes[command] ?? ""}`);
process.exit(1);
