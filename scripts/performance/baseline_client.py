"""Measured native stdio client for synthetic baseline runs (Linux /proc)."""
import json
import os
from pathlib import Path
import selectors
import subprocess
import time

from temporal_body_selection import Client


class BaselineClient(Client):
    def __init__(self, binary, store, trace, encoder, counter=None):
        store.mkdir(parents=True, exist_ok=True)
        self.trace, self.encoder, self.serial = trace, encoder, 0
        env = {k: v for k, v in os.environ.items()
               if not k.startswith(("KMP_", "KERNEL_")) and k != "LD_PRELOAD"}
        env.update(KMP_MCP_BACKEND="embedded", KMP_MCP_DATA_DIR=str(store),
                   KMP_VIEWER_ADDR="127.0.0.1:0", XDG_DATA_HOME=str(store / "xdg"),
                   XDG_STATE_HOME=str(store / "state"),
                   RUST_LOG="kmp_mcp=info,kmp_mcp::query_phases=debug")
        if counter:
            env["LD_PRELOAD"] = str(counter)
        self.stderr = (store / "stderr.log").open("w")
        start = time.perf_counter_ns()
        self.process = subprocess.Popen([str(binary)], stdin=subprocess.PIPE,
            stdout=subprocess.PIPE, stderr=self.stderr, text=True, bufsize=1,
            env=env, cwd=store)
        self.selector = selectors.DefaultSelector()
        self.selector.register(self.process.stdout, selectors.EVENT_READ)
        try:
            self.rpc("initialize", {"protocolVersion": "2024-11-05", "capabilities": {},
                "clientInfo": {"name": "kmp-performance-baseline", "version": "1"}})
        except BaseException:
            self.close()
            raise
        self.startup_ms = (time.perf_counter_ns() - start) / 1e6

    def resources(self):
        result = super().resources()
        io = dict(line.split(":", 1) for line in
                  Path(f"/proc/{self.process.pid}/io").read_text().splitlines())
        result.update({name: int(value) for name, value in io.items()})
        return result

    def rpc(self, method, params):
        # Token counting is client-side, after the measured native round trip.
        before = self.resources()
        result, row = super().rpc(method, params)
        after = self.resources()
        row["proc_io_delta"] = {key: after[key] - before[key]
            for key in ("rchar", "wchar", "syscr", "syscw", "read_bytes", "write_bytes")}
        wire = json.dumps(row["response"], ensure_ascii=False, separators=(",", ":"))
        row["compact_response_tokens"] = len(self.encoder.encode(wire, disallowed_special=()))
        # The base trace is the untouched native measurement. Enrichment is a
        # separate record so token accounting cannot contaminate native time.
        self.trace.write(json.dumps({"accounting_for_id": self.serial,
            "proc_io_delta": row["proc_io_delta"],
            "compact_response_tokens": row["compact_response_tokens"]}) + "\n")
        self.trace.flush()
        return result, row
