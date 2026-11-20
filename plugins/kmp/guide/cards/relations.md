Link memories so a later reader learns why. In a new packet, a link goes in
the record's `connect_to`: the containing memory is `from`, `ref` is `to`.
Between memories that already exist, use top-level `relations` (`from`,
`rel`, `to`); it writes only the link and its evidence.

```json
{"id":"retry","kind":"decision","summary":"Retry the cache.","evidence":"D1: retry because R1 failed.","connect_to":[{"ref":"@failure","rel":"chosen_because","class":"motivational","why":"The failure motivated the retry.","evidence":"D1 names R1's failure as its reason."}]}
```

`ref` is an exact local id (`"failure"` or `"@failure"`) or a returned
canonical ref. Omit `class` when the relation allows one; a rejected class
reports `allowed_values`. Every non-structural link needs its own `why` and
`evidence`. `relation_vocabulary` lists each relation as `name (quality;
classes; when to use)`: pick the richest one the source proves.

- approval -> `authorizes` -> permitted action (not proof it ran);
- claim -> `verified_by` -> the check that verified it;
- corrected fact -> `corrects` -> earlier fact;
- replacement -> `supersedes` -> the **entire** old memory, marked SUPERSEDED.

For a partial correction keep unrelated facts; do not supersede a whole report.
Rich links answer `needs_review` first (topic `write`).

More: `guide:kmp-agent:example:decision-history`.
