"""KMP memory bench (`memory_bench`): decides roadmap steps with numbers, always on the
operator's path — release binary, MCP stdio and disposable stores.

Contracts shared by every part of the bench live in SCHEMAS.md beside this file;
the data types that implement them live in `domain/`.
"""

# Part of every cache key and of the comparability check: bump it whenever the
# driver, the scoring rules or a contract in SCHEMAS.md changes meaning.
BENCH_VERSION = 'kmp.memory_bench.v4'
