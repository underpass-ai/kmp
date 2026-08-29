# OBS evidence capture harness

This harness records the KMP Embedded campaign from real applications:

- a real MCP client running inside a `script(1)` PTY shown by GNOME Terminal;
- the real ChronoLoom page in a clean Chromium profile, including its live
  `/api/view?since=...` long poll;
- OBS Studio 30 controlling the recording over authenticated WebSocket 5.

It never opens the user's desktop. A disposable Xvfb server has two screens:
screen 0 is the 1920×1080 picture with only Terminal and Chromium; OBS runs on
screen 1 and records screen 0 through the bundled XSHM input. The browser
profile, X11 cookie, viewer capability and OBS password are run-scoped and are
removed or redacted after shutdown.

## Run the technical smoke

Build KMP first, then run:

```bash
cargo build -p kmp-mcp --locked
bash campaign/embedded-launch/obs-harness/scripts/doctor.sh
bash campaign/embedded-launch/obs-harness/scripts/run-capture.sh \
  campaign/embedded-launch/obs-harness/scenarios/technical-smoke.json
```

`run-capture.sh` prints the absolute evidence directory. Pass `--promote` only
for an approved campaign scenario; that additionally copies the verified raw
OBS recording to:

```text
campaign/embedded-launch/evidence-pack/capture/raw/<scenario-id>.mkv
campaign/embedded-launch/evidence-pack/capture/promoted/<scenario-id>.json
```

The promoted JSON is the stable adapter for the campaign manifest. It binds the
chosen run directory and hashes the raw recording, PTY, MCP wire, lifecycle,
store, browser revisions, network observations, OBS config/logs and run-level
evidence manifest; consumers never guess which UTC run is current. Every path
in that adapter is a POSIX path relative to the repository root. Consumers
resolve it against their own checkout, reject absolute paths and reject `..` or
symlink escapes.

The three production ids are `fresh-process-same-why`,
`two-processes-one-memory`, and `keep-the-wrong-turn`. Marketing owns their
scene and zoom schedule in `campaign/embedded-launch/edl.json`; this harness
owns capture truth. Master 2 must say PROCESS A/B. A deterministic incident
fixture must be visibly labelled `DETERMINISTIC PRODUCT FIXTURE`.

## Evidence contract

Each run retains:

```text
obs-recording.mkv             OBS raw picture
pty.typescript               exact terminal byte stream
pty.timing                   script(1) timing file
terminal-events.jsonl        plain, monotonic terminal presentation events
tool-calls.jsonl             MCP wire requests/responses plus wire hashes
process-lifecycle.json        client/server PIDs, binary and commit identity
stores.json                  isolated store inventory and hashes
viewer-revisions.jsonl       Chromium-observed /api/view long-poll revisions
browser-network.jsonl        sanitized CDP request/response evidence
window-tree.txt              X11 window identities on the isolated screen
obs-websocket.jsonl          OBS requests, responses and output events
obs-scene-schedule.jsonl     EDL scene requests and monotonic timing
edl.json / edl.sha256        exact schedule contract used by this run
audio-contract.json          exact cue/anchor contract used by this run
anchors.jsonl                edit/audio anchors tied to wire or revision hashes
audio-cues.json              frame-safe cue times resolved from those anchors
clock-map.json               monotonic-ns to encoded-picture PTS mapping
readability-preflight.json   390 px scene raster diagnostic; not panel approval
review-frames/               full-size and 390 px frames for every EDL scene
obs-config/                  exact isolated OBS profile and scene collection
obs.stdout.log / obs.stderr.log / obs-studio/logs/
ffprobe.json                 raw stream identity
verification.json            executable gates and result
evidence-manifest.json       SHA-256 closure over the evidence pack
```

Secret-shaped keys and the ChronoLoom `k` capability are replaced with a
redaction object that includes the SHA-256 of the original value. The raw wire
line hash remains, so a reviewer can distinguish an exact payload from an
explicitly redacted one. The ephemeral clear-text capabilities never survive
finalization.

Promotion and campaign validation both scan the complete run for surviving
`*.private` files, clear ChronoLoom capabilities, OBS passwords or handshake
responses, browser credential databases, private keys and common token formats.
Only non-secret content hashes and the literal OBS value `redacted` survive.
Password fingerprints are deliberately discarded with the password: the
`auth_required` and `cleartext_retained` fields prove the security boundary
without leaving a reusable correlation handle.

OBS is the picture authority, not the event authority. MCP JSONL, PTY timing,
browser revision events and OBS WebSocket timestamps share `CLOCK_MONOTONIC`
nanoseconds, allowing the edit and audio teams to align a cue without treating
a reconstructed screen state as a product event.

Every focus scene contains two live XSHM crops from the same isolated screen:
the requested real window is scaled as the primary picture and the other real
window remains identifiable as an inset. No terminal or browser pixels are
rebuilt in HTML. `run-capture.sh` executes `masters[].obs_schedule` from the
captured `edl.json`; its requests and timing are part of the evidence closure.

`process-lifecycle.json` binds the binary and commit separately from checkout
cleanliness. It discloses `repository.worktree_dirty`, the status/path list and
a stable path-list hash. A dirty campaign worktree is evidence, not a failure,
when the commit and binary SHA-256 still match the reviewed build.

The OBS scheduler owns both scene changes and the picture boundary. The
isolated profile pins x264 `zerolatency`, disables B-frames and look-ahead, and
stops directly against the observed `RecordStateChanged/STARTED` monotonic
clock. Verification checks those encoder settings and rejects a raw duration
more than one 30 fps frame away from the EDL.

OBS desktop audio is disabled. Product sound and campaign music consume only
`anchors.jsonl`, `audio-cues.json` plus `clock-map.json`; nominal cues are
resolved to the first safe encoded frame at or after visible evidence.
Navigation, long-poll, typing, startup,
exit, room tone and OS notifications are never cue sources. The 390 px raster
preflight intentionally reports `pending_5_of_5`: it prepares the required
muted human panel and cannot approve it.
