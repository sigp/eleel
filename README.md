Electric Eel (`eleel`)
====

This is a multiplexer for Ethereum execution clients. It allows multiple consensus
clients to connect to a single execution client endpoint.

There are several limitations:

- Implementation is WIP.
- One consensus node must be nominated as the controlling node.
- No payload building -- `eleel` will never send payload attributes to the EL: use a builder.

`eleel` is API-compatible with [`openexecution`][openexecution], rewritten in
Rust to take advantage of the stronger type-system and to reuse components from
[Lighthouse][].

[openexecution]: https://github.com/TennisBowling/openexecution
[Lighthouse]: https://github.com/sigp/lighthouse

## License

Copyright Sigma Prime 2023 and contributors.

Licensed under the terms of the Apache 2.0 license.
