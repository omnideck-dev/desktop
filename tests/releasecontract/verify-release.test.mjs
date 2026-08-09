import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { verifyReleaseDirectory } from "./verify-release.mjs";

const version = "v1.2.3-alpha.4";
const bareVersion = version.slice(1);
const artifacts = [
  ["nsis", `Omnideck_${bareVersion}_x64-setup.exe`, "x64"],
  ["dmg", `Omnideck_${bareVersion}_aarch64.dmg`, "arm64"],
  ["appimage", `Omnideck_${bareVersion}_amd64.AppImage`, "x64"],
  ["deb", `Omnideck_${bareVersion}_amd64.deb`, "x64"],
  ["rpm", `Omnideck-${bareVersion}-1.x86_64.rpm`, "x64"],
];

function fixture(format, architecture) {
  const contents = Buffer.alloc(2048);
  if (format === "nsis") contents.write("MZ", 0, "ascii");
  if (format === "dmg") contents.write("koly", contents.length - 512, "ascii");
  if (format === "appimage") {
    contents.set([0x7f, 0x45, 0x4c, 0x46], 0);
    contents[5] = 1;
    contents.writeUInt16LE(architecture === "x64" ? 62 : 183, 18);
  }
  if (format === "deb") contents.write("!<arch>\n", 0, "ascii");
  if (format === "rpm") contents.set([0xed, 0xab, 0xee, 0xdb], 0);
  return contents;
}

async function makeRelease() {
  const root = await mkdtemp(path.join(os.tmpdir(), "omnideck-release-contract-"));
  for (const [format, name, architecture] of artifacts) {
    const contents = fixture(format, architecture);
    const digest = createHash("sha256").update(contents).digest("hex");
    await writeFile(path.join(root, name), contents);
    await writeFile(path.join(root, `${name}.sha256`), `${digest}  ${name}\n`);
  }
  return root;
}

test("accepts the complete desktop package matrix with matching checksums", async (t) => {
  const root = await makeRelease();
  t.after(() => rm(root, { recursive: true, force: true }));
  const report = await verifyReleaseDirectory({ directory: root, version });
  assert.equal(report.result, "pass");
  assert.equal(report.artifactCount, 5);
  assert.deepEqual(
    new Set(report.artifacts.map(({ platform }) => platform)),
    new Set(["windows", "macos", "linux"]),
  );
});

test("rejects a package whose checksum does not match", async (t) => {
  const root = await makeRelease();
  t.after(() => rm(root, { recursive: true, force: true }));
  const name = `Omnideck_${bareVersion}_x64-setup.exe`;
  await writeFile(path.join(root, `${name}.sha256`), `${"0".repeat(64)}  ${name}\n`);
  await assert.rejects(verifyReleaseDirectory({ directory: root, version }), /checksum mismatch/);
});

test("rejects a missing artifact", async (t) => {
  const root = await makeRelease();
  t.after(() => rm(root, { recursive: true, force: true }));
  await rm(path.join(root, `Omnideck-${bareVersion}-1.x86_64.rpm`));
  await rm(path.join(root, `Omnideck-${bareVersion}-1.x86_64.rpm.sha256`));
  await assert.rejects(verifyReleaseDirectory({ directory: root, version }), /does not match/);
});

test("rejects an AppImage with the wrong executable architecture", async (t) => {
  const root = await makeRelease();
  t.after(() => rm(root, { recursive: true, force: true }));
  const name = `Omnideck_${bareVersion}_amd64.AppImage`;
  const contents = fixture("appimage", "arm64");
  const digest = createHash("sha256").update(contents).digest("hex");
  await writeFile(path.join(root, name), contents);
  await writeFile(path.join(root, `${name}.sha256`), `${digest}  ${name}\n`);
  await assert.rejects(verifyReleaseDirectory({ directory: root, version }), /wrong ELF architecture/);
});

test("rejects a corrupted/truncated artifact", async (t) => {
  const root = await makeRelease();
  t.after(() => rm(root, { recursive: true, force: true }));
  const name = `Omnideck_${bareVersion}_amd64.deb`;
  const contents = Buffer.alloc(10); // too small, not even a real ar archive
  const digest = createHash("sha256").update(contents).digest("hex");
  await writeFile(path.join(root, name), contents);
  await writeFile(path.join(root, `${name}.sha256`), `${digest}  ${name}\n`);
  await assert.rejects(verifyReleaseDirectory({ directory: root, version }), /unexpectedly small/);
});
