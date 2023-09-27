```
Ethereum execution engine multiplexer

Usage: eleel [OPTIONS] --ee-jwt-secret <PATH> --controller-jwt-secret <PATH> --client-jwt-secrets <PATH>

Options:
      --listen-address <IP>
          Listening address for the HTTP server
          
          [default: 127.0.0.1]

      --listen-port <PORT>
          Listening port for the HTTP server
          
          [default: 8552]

      --ee-url <URL>
          Primary execution engine to be shared by connected consensus nodes
          
          [default: http://localhost:8551]

      --ee-jwt-secret <PATH>
          Path to the JWT secret for the primary execution engine

      --controller-jwt-secret <PATH>
          Path to the JWT secret for the controlling consensus client

      --client-jwt-secrets <PATH>
          Path to TOML file of JWT secrets for the non-controlling consensus clients.
          
          See docs for TOML file format.

      --new-payload-cache-size <N>
          Number of recent newPayload messages to cache in memory
          
          [default: 64]

      --fcu-cache-size <N>
          Number of recent forkchoiceUpdated messages to cache in memory
          
          [default: 64]

      --payload-builder-cache-size <N>
          Number of payload attributes and past payloads to cache in memory
          
          [default: 8]

      --payload-builder-extra-data <STRING>
          Extra data to include in produced blocks
          
          [default: Eleel]

      --justified-block-cache-size <N>
          Number of justified block hashes to cache in memory
          
          [default: 4]

      --finalized-block-cache-size <N>
          Number of finalized block hashes to cache in memory
          
          [default: 4]

      --fcu-matching <NAME>
          Choose the type of matching to use before returning a VALID fcU message to a client
          
          [default: loose]

          Possible values:
          - exact:     match head/safe/finalized from controller exactly
          - loose:     match head and sanity check safe/finalized
          - head-only: match head and ignore safe/finalized (dangerous)

      --network <NAME>
          Network that the consensus and execution nodes are operating on
          
          [default: mainnet]

      --new-payload-wait-millis <MILLIS>
          Maximum time that a consensus node should wait for a newPayload response from the cache.
          
          We expect that the controlling consensus node and primary execution node will take some time to process requests, and that requests from consensus nodes could arrive while this processing is on-going. Using a timeout of 0 will often result in a SYNCING response, which will put the consensus node into optimistic sync. Using a longer timeout will allow the definitive (VALID) response from the execution engine to be returned, more closely matching the behaviour of a full execution engine.
          
          [default: 2000]

      --new-payload-wait-cutoff <NUM_BLOCKS>
          Maximum age of a payload that will trigger a wait on `newPayload`
          
          Payloads older than this age receive an instant SYNCING response. See docs for `--new-payload-wait-millis` for the purpose of this wait.
          
          [default: 64]

      --fcu-wait-millis <MILLIS>
          Maximum time that a consensus node should wait for a forkchoiceUpdated response from the cache.
          
          See the docs for `--new-payload-wait-millis` for the purpose of this timeout.
          
          [default: 1000]

      --body-limit-mb <MEGABYTES>
          Maximum size of JSON-RPC message to accept from any connected consensus node
          
          [default: 128]

  -h, --help
          Print help (see a summary with '-h')
```
