# experiments

A collection of small projects and experiments.

## Projects

| Project | Description |
|---------|-------------|
| **[eth-vanity](eth-vanity/)** | High-performance Ethereum vanity address generator. Generates keypairs and checks addresses against patterns (prefix/suffix/contains). CPU multi-threaded by default; optional OpenCL GPU backend. |
| **[safe-vanity](safe-vanity/)** | Mine Safe Account vanity addresses by varying `saltNonce` until the CREATE2-derived proxy address matches a pattern. Rust miner + JS executor for config, verification, and deployment. |

Each project has its own README with build and usage instructions.
