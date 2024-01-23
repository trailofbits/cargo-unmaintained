# Changelog

## 0.3.3

- When checking repository existence, treat a timeout as nonexistence ([#98](https://github.com/trailofbits/cargo-unmaintained/pull/98))
- Upgrade `env_logger` to version 0.11.0 ([dae4c37](https://github.com/trailofbits/cargo-unmaintained/commit/dae4c373b71ee73a8b9fe37f0c95fc617267c0f9))

## 0.3.2

- Distinguish more cases in unmaintained messages ([#66](https://github.com/trailofbits/cargo-unmaintained/pull/66))
- Warn when an ignored package is not depended upon ([#64](https://github.com/trailofbits/cargo-unmaintained/pull/64))

## 0.3.1

- Fix a bug causing ignore feature to not work ([#57](https://github.com/trailofbits/cargo-unmaintained/pull/57))

## 0.3.0

- FEATURE: Check for repository existence, and verify that a package appears in its repository ([#32](https://github.com/trailofbits/cargo-unmaintained/pull/32) and [#37](https://github.com/trailofbits/cargo-unmaintained/pull/37))

## 0.2.1

- Fix a bug causing --tree to fail ([#29](https://github.com/trailofbits/cargo-unmaintained/pull/29))

## 0.2.0

- Do not check for outdated dependencies in archived packages ([#22](https://github.com/trailofbits/cargo-unmaintained/pull/22))
- FEATURE: Add ability to ignore packages ([#20](https://github.com/trailofbits/cargo-unmaintained/pull/20))

## 0.1.2

- Make `windows-sys` an optional dependency ([#15](https://github.com/trailofbits/cargo-unmaintained/pull/15))

## 0.1.1

- Documentation improvements ([#9](https://github.com/trailofbits/cargo-unmaintained/pull/9))
- Fix crates.io description ([#10](https://github.com/trailofbits/cargo-unmaintained/pull/10))

## 0.1.0

- Initial release
