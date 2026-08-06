# Vendored third-party assets

The viewer embeds its UI into the binary; nothing is fetched at runtime and
`npm` is never invoked — vendored files are obtained as plain registry
artifacts (no lifecycle scripts execute, which was the propagation vector of
the 2025–2026 "Shai-Hulud" npm worm waves) and pinned here by hash.

## pixi.js 8.19.0 — `pixi.min.js`

| Field | Value |
|:------|:------|
| Package | `pixi.js` 8.19.0 (published 2026-06-04) |
| Source | `https://registry.npmjs.org/pixi.js/-/pixi.js-8.19.0.tgz` |
| Tarball integrity (registry-declared, reproduced locally) | `sha512-pq1O6emA/GFjjeF+8d3Pb5t7knD8FsnfWGqQcRjYjsqFZ7QdzG1XgjLDUu0DFJRbafjV5+g8iNLFBx0b9649lg==` |
| File | `package/dist/pixi.min.js` → `ui/vendor/pixi.min.js` |
| File hash | `sha256 83b2d7edf27bb77460f5f5f5e25cd73c91b77a53f44c80ac63096d6c0b5cfda7` |
| File | `package/dist/packages/unsafe-eval.min.js` → `ui/vendor/pixi-unsafe-eval.min.js` (no-eval shader path; the viewer's CSP forbids `unsafe-eval`) |
| File hash | `sha256 37bb398ade979f9fa0251c66a5f3093bdc1318b4b21887e0aad6cc3ae0368193` |
| License | MIT (`PIXI-LICENSE`) |

### Supply-chain verification (2026-08-06)

Checked before vendoring, all clean:

- **OSV.dev**: zero records for `pixi.js` (all versions), for `pixi.js@8.19.0`,
  and for the `pixijs` name — OSV aggregates the `MAL-*` malicious-package
  database that catalogued every version compromised by the Shai-Hulud waves
  (2025-09, 2025-11, 2026 Keyv wave).
- **GitHub Advisory Database**: zero advisories affecting `pixi.js`.
- **Package manifest**: no `install`/`preinstall`/`postinstall` scripts.
- **Published compromised-package lists** (CISA 2025-09-23, Wiz, Datadog,
  Unit 42, Aikido): `pixi.js`/`pixijs` absent from all waves.
- **Tarball integrity**: the downloaded artifact reproduces the
  registry-declared sha512 exactly (recorded above).
- **Bundle inspection**: plain ASCII IIFE assigning the `PIXI` global; no
  `eval(`; no network calls added by us — the viewer's CSP
  (`default-src 'none'; script-src 'self'; connect-src 'self'`) would block
  any egress a compromised bundle attempted anyway.

### To upgrade

1. Pick the target version; re-run the OSV + GitHub advisory queries and
   re-check the wave lists for that version.
2. `curl` the tarball, verify the registry `dist.integrity` sha512 locally.
3. Extract `package/dist/pixi.min.js`, record its sha256 here, replace the
   file, update this table.
