# Vendored render engine

ChronoLoom embeds its renderer in the binary and MCP App resource. No CDN,
network dependency, package installation or build step runs at viewer startup.
Three.js replaces PixiJS because orbiting about planes requires a depth buffer,
a perspective camera, ray casting and transparent surfaces in actual 3D.
The flat camera uses the same geometry.

| Artifact | Pin |
| --- | --- |
| Package | three 0.180.0 |
| Source | https://registry.npmjs.org/three/-/three-0.180.0.tgz |
| Registry and reproduced integrity | `sha512-o+qycAMZrh+TsE01GqWUxUIKR1AL0S8pq7zDkYOQw8GqfX8b8VoCKYUoHbhiX5j+7hr8XsuHDVU6+gkQJQKg9w==` |
| Bundle | `three.min.js`, 720493 bytes |
| Bundle SHA-256 | `45d8f97107302c103faebbe44ac4f1f3b2124e8ac1163998603dc3d8f452cc73` |
| License | MIT, `THREE-LICENSE` (matches the registry tarball) |
| Bundler | esbuild 0.25.9 |
| Entry | `three-entry.js`: Three.js and its OrbitControls adapter |

On 2026-09-06 the registry tarball's SHA-512 and all 1,117 package files used
as bundle inputs were verified against the installed source, including its license.
OSV's npm/three/0.180.0 query returned [].

To reproduce, obtain the pinned registry artifacts without lifecycle scripts,
verify their integrity, and run esbuild 0.25.9 with three 0.180.0 on its module
search path:

```sh
esbuild three-entry.js --bundle --format=iife --global-name=KMP_THREE --minify --outfile=three.min.js
sha256sum three.min.js
```

`node --test ui/loom-scene.test.js` checks the embedded bundle digest and actual
camera geometry. The source uses no shader eval; the loopback CSP continues to
refuse `unsafe-eval`. The same bundle is inlined for the negotiated MCP App.
