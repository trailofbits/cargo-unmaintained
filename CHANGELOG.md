# Changelog

## 1.2.1

- Update lockfile. Needed to get `cargo-unmaintained` to build with Rust 1.8.0. ([275c9db](https://github.com/trailofbits/cargo-unmaintained/commit/275c9db172210d8b2e9decde315c1b1bdd49e8ab))

## 1.2.0

- FEATURE: Consider when dependencies were published. Don't report a package as unmaintained just because an incompatible upgrade exists for one of its dependencies, but that upgrade is less than 365 days old (the default). ([#311](https://github.com/trailofbits/cargo-unmaintained/pull/311))

## 1.1.0

- FEATURE: Allow GitHub token to be passed in `GITHUB_TOKEN` environment variable; warn when neither `GITHUB_TOKEN_PATH` nor `GITHUB_TOKEN` is set ([9b39e32](https://github.com/trailofbits/cargo-unmaintained/commit/9b39e320b263910b2a4dc57f0fe6dd6027d7f6fd))
- Don't check dependencies in private registries ([#281](https://github.com/trailofbits/cargo-unmaintained/pull/281))
- Don't consider whether workspace members are unmaintained ([3f9836b](https://github.com/trailofbits/cargo-unmaintained/commit/3f9836bf53d2715a62820c9f7b0164e9dedb8abd))
- Update `crates-index` to version `3.0` ([#300](https://github.com/trailofbits/cargo-unmaintained/pull/300))

## 1.0.2

- Update dependencies, including `gix` to version 0.63.0 ([#269](https://github.com/trailofbits/cargo-unmaintained/pull/269))

## 1.0.1

- Don't emit duplicate errors when cloning a repository fails ([#251](https://github.com/trailofbits/cargo-unmaintained/pull/251))

## 1.0.0

- Up `curl` timeout to 60 seconds. (10 seconds was a little too aggressive.) ([79905a8](https://github.com/trailofbits/cargo-unmaintained/commit/79905a8e1b373035e13fddd3b850cda0362e6eb3))
- Eliminate reliance on `octocrab`. (The tests still use `octocrab`, though.) ([#193](https://github.com/trailofbits/cargo-unmaintained/pull/193))
- Cache repositories on disk between runs ([33585c5](https://github.com/trailofbits/cargo-unmaintained/commit/33585c5520f9e2ec83fdb8bc34057a12d1a9ab67)
  and [edb06c7](https://github.com/trailofbits/cargo-unmaintained/commit/edb06c77d90dbf1792849c89cc68f58f16c70ae5))
- BREAKING CHANGE: Remove `--imprecise` option ([addffbc](https://github.com/trailofbits/cargo-unmaintained/commit/addffbc3742981bb6c4a68bb47d1ea97e4930d60))
- BREAKING CHANGE: Rename `lock_index` feature to `lock-index` ([#222](https://github.com/trailofbits/cargo-unmaintained/pull/222))
- Add "No unmaintained packages found" message ([#223](https://github.com/trailofbits/cargo-unmaintained/pull/223))
- Silence "failed to parse" warnings ([86221f8](https://github.com/trailofbits/cargo-unmaintained/commit/86221f8b0eafcf1a5ccd4a1f0e975ced11663a01))

## 0.4.0

- A package passed to `-p` is no longer required to be a dependency. Passing any `NAME` in `cargo unmaintained -p NAME` will cause the package to be downloaded from `crates.io` and checked. ([#136](https://github.com/trailofbits/cargo-unmaintained/pull/136))

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
