// The updater wiring in tauri.conf.json: gosubstrate.com's census update route and a minisign public key. A wrong
// endpoint or key strands every installed copy, so it is pinned here. The key's private half is kept the same way as
// the other Substrate apps' (Substrate vault Apps/README.md, "Signing keys"); CI signs with it only when the repo
// secret exists (.github/workflows/release.yml).
import { expect, test } from "bun:test";
import conf from "../src-tauri/tauri.conf.json";
import extra from "../src-tauri/tauri.updater.conf.json";

test("updater endpoint is gosubstrate.com's census route", () => {
  expect(conf.plugins.updater.endpoints).toEqual([
    "https://gosubstrate.com/api/apps/census/update/{{target}}/{{arch}}/{{current_version}}",
  ]);
});

test("pubkey is a base64 minisign public key", () => {
  const text = Buffer.from(conf.plugins.updater.pubkey, "base64").toString("utf8");
  const [comment, key] = text.trim().split("\n");
  expect(comment).toMatch(/^untrusted comment: minisign public key: [0-9A-F]{16}$/);
  const raw = Buffer.from(key, "base64");
  expect(raw.length).toBe(42);
  expect(raw.subarray(0, 2).toString()).toBe("Ed");
});

test("updater bundles are made only through the CI config overlay", () => {
  // tauri build fails without TAURI_SIGNING_PRIVATE_KEY when createUpdaterArtifacts is on, so the base config keeps
  // it off and CI adds the overlay only when the secret is set.
  expect((conf.bundle as Record<string, unknown>).createUpdaterArtifacts).toBeUndefined();
  expect(extra).toEqual({ bundle: { createUpdaterArtifacts: true } });
});
