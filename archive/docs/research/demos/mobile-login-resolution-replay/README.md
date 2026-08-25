# Mobile Login Resolution Replay Demo

Static interactive artifact for the live MCP mobile login incident run.

Open `index.html` directly in a browser. There is no build step, backend, CDN,
or package install. The incident data is stored in `incident-data.js` so the demo
works from a local file as well as from static hosting.

The demo supports:

- scoped replay views: core incident, distractor included, wrong turns, final
  path, and decision zoom.
- session filters for `release`, `mobile`, `auth`, `coordinator`,
  `mitigation`, and `infra`.
- node inspection with evidence and incoming/outgoing relations.
- edge inspection with relation class, confidence, and rationale.
- pan and zoom on the SVG graph.
- deep links for article screenshots: `#core`, `#distractor`, `#wrong`,
  `#final`, and `#decision`.
