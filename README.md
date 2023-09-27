Electric Eel (`eleel`)
====

Eleel is a multiplexer for Ethereum execution clients. It allows multiple consensus
clients to connect to a single execution client.

It is suitable for monitoring and analytics but **should not** be used to run
validators and **will** cause you to miss block proposals if you attempt this.

Eleel is written in Rust and makes use of components from [Lighthouse][].

## Build from source

```
cargo install --release --locked
```

A binary will be installed to `~/.cargo/bin/eleel`.

## Getting started

Eleel needs to connect to a _real_ execution node, e.g. Geth/Nethermind/Besu. You
should set up an execution node and take note of the JWT secret it uses. We'll provide it to
Eleel's `--ee-jwt-secret` flag.

Eleel also needs a _controlling consensus node_ (e.g. Lighthouse) which is responsible for driving
the execution node. The controlling node is _in charge_, and its messages will be sent
directly to the execution node. The controller connects to Eleel using the `/canonical` path. There
is a second JWT secret for authenticating the controller to Eleel, which is set using the
`--controller-jwt-secret` flag.

You can generate a new JWT secret using this command:

```
openssl rand -hex 32 | tr -d "\n"
```

In addition to the single controller node, Eleel also supports _multiple_ consensus clients,
which can be authenticated using any of a collection of JWT secrets specified in a TOML config
file. There are a lot of JWT secrets! An example TOML file can be seen below:

```toml
[secrets]
node1 = "c259fb249f7fa1882b1d4150ace73c1023aba4f6267b29a871ad5c9adc7a543a"
node2 = "fb6073f77160f9a7ce11190d3612e841daea2e7319a59e1d82a8804e9fa193ee"
```

The identifiers `node1` and `node2` are _key IDs_ which are used by Eleel to decide which secret
to use when authenticating a request. It does this by examining the `key` field of the claim
(distinct from the standard JWT key-id). If the `key` is not set by the client then Eleel
tries all of the keys in a random order looking for a match (slow).

Putting this all together, here's an example of Eleel sharing a Geth node between two Lighthouse
nodes:

Eleel, connected to Geth on port 8551 and serving the consensus nodes on port 8552:

```
eleel \
  --ee-jwt-secret /tmp/execution.jwt \
  --ee-url "http://localhost:8551" \
  --controller-jwt-secret /tmp/controller.jwt \
  --client-jwt-secrets /tmp/client-secrets.toml \
  --listen-port 8552
```

Geth, using the same JWT secret that Eleel uses to connect to it:

```
geth --authrpc.jwtsecret /tmp/execution.jwt --authrpc.port 8551
```

Lighthouse running as the controller and connected to Eleel's `/canonical` endpoint:

```
lighthouse bn \
  --execution-endpoint "http://localhost:8552/canonical" \
  --execution-jwt "/tmp/controller.jwt"
```

Lighthouse running as a client connected to Eleel's `/` endpoint:

```
lighthouse bn \
  --execution-endpoint "http://localhost:8552" \
  --execution-jwt "/tmp/node1.jwt" \
  --execution-jwt-id "node1"
```

### CLI reference

For full CLI options see [`./docs/cli-reference.md`](./docs/cli-reference.md), or run
`eleel --help`.

## Block Building

Eleel does not build valid execution blocks, but will build _invalid_ dummy execution
payloads. This is designed for use with [blockdreamer][blockdreamer] only and
**will cause you to propose invalid blocks** if used by a validator client.

## License

Copyright Sigma Prime 2023 and contributors.

Licensed under the terms of the Apache 2.0 license.

[Lighthouse]: https://github.com/sigp/lighthouse
[blockdreamer]: https://github.com/blockprint-collective/blockdreamer
