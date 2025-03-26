Electric Eel (`eleel`)
====

Eleel is a multiplexer for Ethereum execution clients. It allows multiple consensus
clients to connect to a single execution client.

It is suitable for monitoring and analytics but **should not** be used to run
validators and **will** cause you to miss block proposals if you attempt this.

Eleel is written in Rust and makes use of components from [Lighthouse][].

## Build from source

```
 cargo install --locked --path .
```

A binary will be installed to `~/.cargo/bin/eleel`.

### Build docker images

### Using `bake`

See https://docs.docker.com/build/building/multi-platform/.

`docker buildx bake -f docker-bake.hcl`

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
to use when authenticating a request. It does this by examining the `id` field of the claim
(distinct from the standard JWT key-id). If the `id` is not set by the client then Eleel
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

## CLI reference

For full CLI options see [`./docs/cli-reference.md`](./docs/cli-reference.md), or run
`eleel --help`.

## HTTP API reference

- `POST /`: JSON-RPC endpoint for multiplexed consensus clients. Requires a JWT
  token authenticated by one of the secrets in the `--client-jwt-secrets` file.
- `POST /canonical`: JSON-RPC endpoint for the controlling consensus client. Requires a JWT
  token authenticated by the JWT secret provided to the
  `--controller-jwt-secret` flag.
- `curl -X GET "http://localhost:8552/health" -v`: health endpoint returning a 200 OK whenever Eleel is running.

## Logging

Eleel only prints logs when the `RUST_LOG` environment variable is set. We recommend running
with debug logs enabled like so:

```
RUST_LOG=eleel=debug eleel ...
```

More information about JWT authentication can be seen at `trace` level.

## Block Building

Eleel does not build valid execution blocks, but will build _invalid_ dummy execution
payloads. This is designed for use with [blockdreamer][blockdreamer] only and
**will cause you to propose invalid blocks** if used by a validator client.

If running Lighthouse as the controller, you should ensure it is configured to send payload
attributes every slot using these flags:

```
lighthouse bn \
  --always-prepare-payload \
  --prepare-payload-lookahead 8000 \
  --suggested-fee-recipient 0xffffffffffffffffffffffffffffffffffffffff
```

> Note: If both consensus clients are using the same port (for example, Lighthouse uses p2p port 9000 by default), the second consensus client will require an additional flag `--port`.

Some other consensus clients provide similar flags.

## License

Copyright Sigma Prime 2023 and contributors.

Licensed under the terms of the Apache 2.0 license.

[Lighthouse]: https://github.com/sigp/lighthouse
[blockdreamer]: https://github.com/blockprint-collective/blockdreamer
