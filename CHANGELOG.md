# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

# [0.5.1] - 2024-02-06

### Changed

- Added support in `AstroportPool` for custom pool type `astroport-pair-xyk-sale-tax`.

# [0.5.0] - 2023-10-30

### Added

- Added support for Astroport's passive concentrated liquidity pool type.

### Changed

- Changed `AstroportPool` to use the new Astroport liquidity manager contract which supports directly passing a min_out for provide and withdraw liquidity.
  - This is a breaking change as `AstroportPool::new` now takes an argument `liquidity_manager: Addr`.
- Changed fn `get_pool_for_lp_token` in `Pool` to take an argument `astroport_liquidity_manager: Option<Addr>`. This is needed to support using the new liquidity manager contract.
- Bump `cw-it` to `0.2.2`

# [0.4.1] - 2023-10-25

### Added

- fn `claim_rewards` on `AstroportStaking` now unwraps any CW20-wrapped native tokens claimed from the staking contract.
  - See the astroport [native-coin-wrapper](https://github.com/astroport-fi/astroport-core/tree/main/contracts/periphery/native-coin-wrapper) contract.

# [0.4.0] - 2023-09-26

### Changed

- Changed field `astro_addr: Addr` to `astro_token: AssetInfo` on struct `AstroportStaking`.
  - This is a breaking change.
  - This is to support chains where ASTRO is a native token.
- Implemented `Pool::get_pool_for_lp_token` for Astroport.
- Upgraded dependencies
  - Upgraded osmosis-std to 0.19.2
  - Upgraded cw-it to 0.2.1

# [0.3.1] - 2023-07-20

### Changed

Use `StdError::generic_err` instead of constructing a literal `StdError` in `cw-dex/src/error.rs`,
so that we don't have to fill the backtraces field.

## [0.3.0] - 2023-07-17

Note: This relase contains breaking API changes.

### Changed

- Upgraded dependencies
    - Upgraded osmosis-std to 0.16.0
    - Upgraded astroport to 2.8.0
- Removed argument `sender` of function `simulate_swap` of trait `Pool`.
    - This is no longer needed with the new API of Osmosis v16.

## [0.2.0] - 2023-06-06

### Security

- Add argument `min_out: AssetList,` to function `withdraw_liquidity` of trait `Pool`.
  - Note: This is a breaking change.
  - Note: This argument is currently ignored for Astroport as they do not support minimum output amounts for withdrawal liquidity. Support will be added in a future release.

### Added

- Use `min_out` argument in function `withdraw_liquidity` of implementation of trait `Pool` for `OsmosisPool`.
