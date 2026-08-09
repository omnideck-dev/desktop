import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const testRoot = path.dirname(fileURLToPath(import.meta.url));
const validator = path.join(testRoot, "validate-proof.mjs");
const vendor = JSON.parse(
  await readFile(path.join(testRoot, "../../src-tauri/binaries/vendor-manifest.json"), "utf8"),
);

async function fixture(overrides = {}) {
  const root = await mkdtemp(path.join(os.tmpdir(), "omnideck-hardware-proof-"));
  const application = path.join(root, "omnideck-fixture");
  const proof = path.join(root, "proof.json");
  const report = path.join(root, "report.json");
  await writeFile(application, Buffer.alloc(1024, 1));
  await writeFile(
    proof,
    JSON.stringify({
      cliVersion: vendor.version,
      cliCommit: vendor.commit,
      schemaVersion: 4,
      ready: true,
      operations: ["--version", "--json runtime status"],
      mutation: false,
      ...overrides,
    }),
  );
  return { root, application, proof, report };
}

function run(paths, extraArgs = []) {
  return spawnSync(
    process.execPath,
    [validator, "--proof", paths.proof, "--application", paths.application, "--report", paths.report, ...extraArgs],
    { encoding: "utf8" },
  );
}

test("accepts the exact read-only packaged smoke proof", async (t) => {
  const paths = await fixture();
  t.after(() => rm(paths.root, { recursive: true, force: true }));
  const result = run(paths, ["--require-ready"]);
  assert.equal(result.status, 0, result.stderr);
  const report = JSON.parse(await readFile(paths.report, "utf8"));
  assert.equal(report.result, "pass");
  assert.equal(report.proof.mutation, false);
  assert.equal(report.application.size, 1024);
});

test("rejects a smoke proof that reports a mutation", async (t) => {
  const paths = await fixture({ mutation: true });
  t.after(() => rm(paths.root, { recursive: true, force: true }));
  const result = run(paths);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /packaged smoke must remain read-only/);
});

test("rejects a CLI version that doesn't match the vendor manifest", async (t) => {
  const paths = await fixture({ cliVersion: "v0.9.0" });
  t.after(() => rm(paths.root, { recursive: true, force: true }));
  const result = run(paths);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /does not match the vendor manifest/);
});

test("rejects the wrong runtime status schema version", async (t) => {
  const paths = await fixture({ schemaVersion: 3 });
  t.after(() => rm(paths.root, { recursive: true, force: true }));
  const result = run(paths);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /schema version 4/);
});

test("--require-ready rejects a not-ready runtime; without it, not-ready still passes", async (t) => {
  const paths = await fixture({ ready: false });
  t.after(() => rm(paths.root, { recursive: true, force: true }));
  const strict = run(paths, ["--require-ready"]);
  assert.notEqual(strict.status, 0);
  assert.match(strict.stderr, /runtime was not ready/);

  const lenient = run(paths);
  assert.equal(lenient.status, 0, lenient.stderr);
});
