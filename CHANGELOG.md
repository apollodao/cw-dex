# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] - 2023-06-06

### Security

- Add argument `min_out: AssetList,` to function `withdraw_liquidity` of trait `Pool`.
  - Note: This is a breaking change.
  - Note: This argument is currently ignored for Astroport as they do not support minimum output amounts for withdrawal liquidity. Support will be added in a future release.

### Added

- Use `min_out` argument in function `withdraw_liquidity` of implementation of trait `Pool` for `OsmosisPool`.
