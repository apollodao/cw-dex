# cw-dex
CosmWasm abstractions for decentralized exchanges.

This crate defines a set of
traits to abstract out the common behavior of various decentralized exchanges so
that the same contract code can be used to interact with any of them.

The currently supported decentralized exchanges are:
- [Osmosis](cw-dex/src/implementations/osmosis/)
- [Astroport](cw-dex/src/implementations/astroport/)
- [Picasso](cw-dex/src/implementations/picasso/)