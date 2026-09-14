# Writer clocks acceptance fixture

`SOURCE.md` and `WRITER-INSTRUCTIONS.md` are the only materials supplied to a
fresh writer. `driver.py` starts one persistent embedded store and records its
handshake, every native response, and every driver error.

Run the writer interactively. Each stdin line is a JSON object with `tool` and
`arguments`; the driver emits the result before reading the next line. Send
`quit` when finished.

```bash
python3 artifacts/writer-clocks-20260915/driver.py \
  --binary /absolute/path/to/kmp-mcp \
  --trace artifacts/writer-clocks-20260915/attempt.native.jsonl
```

The future trace and its persistent temporary store are intentionally absent.
They record the fresh writer's actual choices and errors. The trace includes
SHA-256 digests for the binary, source, instructions and driver. The source and
driver contain no golden KMP payload or expected response. A later review can
check receipt, neighborhood and clocks, but cannot prove semantic comprehension
or source coverage.
