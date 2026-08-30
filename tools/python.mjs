import { spawnSync } from "node:child_process";

const candidates = process.platform === "win32"
  ? [["python"], ["python3"], ["py", "-3"]]
  : [["python3"], ["python"]];
const args = process.argv.slice(2);
if (args.length === 0) {
  console.error("usage: node tools/python.mjs <script.py> [arguments...]");
  process.exit(2);
}

for (const [command, ...prefix] of candidates) {
  const result = spawnSync(command, [...prefix, ...args], { stdio: "inherit" });
  if (result.error && "code" in result.error && result.error.code === "ENOENT") continue;
  if (result.error) throw result.error;
  process.exit(result.status ?? 1);
}

console.error("Python 3 was not found. Install Python 3 or add py/python/python3 to PATH.");
process.exit(127);
