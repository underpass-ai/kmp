# kmp-memory-api

[KMP](https://github.com/underpass-ai/kmp) is local-first agent memory that
preserves what happened, when and why. This crate is the published consumer
contract of its embedded kernel.

Sibling of [`kmp-plugin-api`](https://crates.io/crates/kmp-plugin-api),
pointing the other way: that crate is what a plugin may know about the kernel,
this one is what an embedding product may know.

It holds plain views, a capability report, an error vocabulary and two traits
— recall and record, kept separate on purpose. No domain aggregates, no ports,
no storage, no transports. A consumer that compiles against this crate alone
can be tested against a stub and then run against any implementation that
honours the contract.

## Versioned by meaning

`CONTRACT_VERSION` moves when the meaning of this surface changes,
independently of the kernel's release number. Two builds of one release can
differ in features, so a consumer that guessed capabilities from a version
string would find out mid-run. Check `ApiCapabilities` at startup instead.

The vocabulary here is the kernel's — abouts, memory, wake, ask, record. A
consuming product maps these to its own terms at its own boundary; nothing of
any consumer's vocabulary appears in this crate.

## License

Apache-2.0.
