## Final measurements after the clock-format fix

These are a fresh run on 0.18.1 with the deterministic timestamp fix. The initial
eight runs remain in `performance-539`; none of their observations are overwritten.
The same fixture equivalence control passed before timing. `binary.json`,
`source.patch` and `source-hashes.json` identify the exact measured code.
Hardware and Cargo configuration match the initial run; no local builds/tests
ran concurrently. The runner now creates and cleans its own temporary stores.

### Native application + serialization

| Fixture | p50 ms before → after | p95 ms before → after | Response bytes before → after | Calls before → after |
|---|---:|---:|---:|---:|
| small | 0.461 → 0.272 | 0.486 → 0.300 | 1,817 → 1,084 | 1 → 1 |
| medium | 19.898 → 3.440 | 30.159 → 3.839 | 60,616 → 11,885 | 8 → 1 |
| high_degree | 543.181 → 83.209 | 619.098 → 84.700 | 2,167,360 → 600,895 | 64 → 1 |
| large_body | 200.428 → 2.295 | 209.312 → 2.874 | 2,133,960 → 11,885 | 8 → 1 |

### Actual loopback HTTP round trips

| Fixture | p50 ms before → after | p95 ms before → after | Response bytes before → after | Calls before → after |
|---|---:|---:|---:|---:|
| small | 0.934 → 0.769 | 1.934 → 0.904 | 2,121 → 1,388 | 1 → 1 |
| medium | 16.646 → 4.041 | 25.001 → 5.138 | 63,048 → 12,190 | 8 → 1 |
| high_degree | 466.986 → 117.926 | 523.786 → 119.385 | 2,186,880 → 601,201 | 64 → 1 |
| large_body | 253.116 → 2.947 | 317.408 → 3.399 | 2,136,408 → 12,190 | 8 → 1 |

Whole-process HTTP peak RSS remains higher: [22064, 25188] KiB before and [44972, 45020] KiB after. This is not per-request allocation accounting. All initial scope limitations still apply.
