# KMP Embedded first campaign

The campaign is governed by five independent sources:

- `campaign.json` is the shared-schema brief and muted beat contract;
- `claims.json` limits every public line to named product evidence;
- `edl.json` is the OBS picture and timing decision list;
- `scenario-contracts.json` fixes the process, tool and proof events each real
  capture must implement;
- `evidence-pack/manifest.json` binds capture, edit, audio and master artifacts.

Final picture must come from OBS Studio window capture. Every master contains a
real PTY terminal and a real Chromium window running ChronoLoom. A terminal
drawn into browser DOM is forbidden. Codex and Claude branding is forbidden
until raw capture proves those real hosts; the current two-process story uses
`Process A` and `Process B`.

The capture adapter writes raw OBS pictures to:

```text
evidence-pack/capture/raw/<master-id>.mkv
```

It must also persist the PTY transcript, exact tool-call JSONL, process
lifecycle, store fingerprints and ChronoLoom revisions described in
`evidence-pack/README.md`. `scripts/demo/record-chronoloom-gifs.js` remains a
browser/product evidence probe; it does not create or imitate a terminal.

No campaign MP4 is checked in before picture lock. Superseded pre-OBS renders
stay outside the tree; the final files under `docs/assets/campaign/kmp-embedded/`
are created only from promoted OBS raws and rebound by the evidence manifest.

Audio is rendered independently while picture is still unlocked:

```bash
python3 campaign/embedded-launch/scripts/render-campaign.py \
  --audio-only tmp/campaign-audio
python3 campaign/embedded-launch/scripts/test_audio_contract.py \
  tmp/campaign-audio
```

That command synthesizes the original Evidence Knot palette, applies the fixed
cue-level staging, writes 48 kHz 24-bit stems and pre-mixes, then runs one
deterministic precontrol stage and two-pass EBU R128 normalization. The
normalizer works toward 5 LU LRA; the release gate remains no greater than 8
LU at -18 LUFS-I ±1 and no greater than -1.5 dBTP. Named transition and
final-hold intervals must decode to per-sample digital zero, and a mono
fold-down must retain the expected energy, true peak and LRA. Determinism is
the SHA-256 of decoded interleaved `s24le` PCM; WAV container hashes are
retained only for artifact integrity. The resulting `audio-evidence.json`
remains explicitly `deterministic_premix_not_picture_locked`.

Final MP4 audio is AAC-LC, 48 kHz stereo. `192 kb/s` is the encoder target,
not a promise about the average bitrate of sparse audio; `ffprobe` records the
observed stream bitrate and the release gate verifies codec, profile, sample
rate, channel count and decoded loudness.

There is exactly one GIF derivative: `docs/assets/kmp-agent-loom.gif`, derived
from the final master 1, `fresh-process-same-why`. It is the only campaign
image embedded in the README.

Validation:

```bash
python3 campaign/embedded-launch/scripts/validate-campaign.py
python3 campaign/embedded-launch/scripts/build-critic-input.py
python3 campaign/embedded-launch/scripts/prepare-mobile-muted-panel.py
python3 campaign/embedded-launch/scripts/build-evidence-manifest.py build
python3 campaign/embedded-launch/scripts/build-evidence-manifest.py check
```

The campaign validator is automated. Critic input is generated only after the
release identity and final master hashes exist. The final manifest check stays
non-zero until OBS capture, release, audio evidence and the required human
panels all exist. The independent critic audits that immutable manifest. Only a
clean `GO` whose input hash still matches may unlock the README GIF and the
post-audit publication manifest.
