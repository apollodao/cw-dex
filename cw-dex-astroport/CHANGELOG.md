# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

# [0.2.0-rc.1] - 2024-05-01

### Changed

- Added support for Astroport native LP tokens.
- Bumped `astroport_v3` dependency alias of `astroport` package to `5.0.0-rc.1-tokenfactory` and renamed it to `astroport_v5`.
- Changed `AstroportStaking` field `lp_token_addr: Addr` to `lp_token: AssetInfo`.
- Changed `AstroportPool` field `lp_token_addr: Addr` to `lp_token: AssetInfo`.
- Changed `AstroportPool` field `liquidity_manager: Addr` to `liquidity_manager: Option<Addr>`.
  - This is to keep backwards compatibility with Astroport pools that use CW20 LP tokens. Note that the field must be set if the LP token is a CW20 token.
