# Changelog

## 1.8.1

- The `--purge` option now removes `$HOME/.cache/cargo-unmaintained` rather than `$HOME/.cache/cargo-unmaintained/v2` ([#565](https://github.com/trailofbits/cargo-unmaintained/pull/565))&mdash;thanks [@Stereco-btc](https://github.com/Stereco-btc)

## 1.8.0

- FEATURE: Add `--purge` option to remove `$HOME/.cache/cargo-unmaintained` ([#551](https://github.com/trailofbits/cargo-unmaintained/pull/551))&mdash;thanks [@fabiocarlos97](https://github.com/fabiocarlos97)
- Unpin `crate-index` dependency and upgrade it to version 3.8 ([#554](https://github.com/trailofbits/cargo-unmaintained/pull/554))

## 1.7.0

- Add note to README.md about why a package could be flagged one day but not the next ([68ce0e0](https://github.com/trailofbits/cargo-unmaintained/commit/68ce0e03285139593d33a5859d85bc8d1146c206))
- FEATURE: Make verbose printing more informative ([#548](https://github.com/trailofbits/cargo-unmaintained/pull/548))

## 1.6.3

- Update dependencies, including `openssl` to version 0.10.70 ([#502](https://github.com/trailofbits/cargo-unmaintained/pull/502))

## 1.6.2

- Eliminate reliance on `once_cell` ([f12bb3a](https://github.com/trailofbits/cargo-unmaintained/commit/f12bb3ad03ce5b5b43424518a2b4bf41268de53b))

## 1.6.1

- Do not consider a package unmaintained because it is stored in a Mercurial repository ([#489](https://github.com/trailofbits/cargo-unmaintained/pull/489))

## 1.6.0

- FEATURE: Add experimental `--json` option to output JSON ([#464](https://github.com/trailofbits/cargo-unmaintained/pull/464))

## 1.5.1

- Clone but do not checkout repositories. **WARNING: This change causes the cache to be rebuilt.** Prior to this change, `cargo-unmaintained` could not handle repositories containing paths not supported by the host filesystem. This bug was observed on Windows (e.g., NTFS). Thanks to [@elopez](https://github.com/elopez) whose suggestions contributed to the fix. ([6ce1f8d](https://github.com/trailofbits/cargo-unmaintained/commit/6ce1f8de9d09b4d41c714cd78480622da5f5f328))
- Update list of [known problems](https://github.com/trailofbits/cargo-unmaintained?tab=readme-ov-file#known-problems) in README.md ([#451](https://github.com/trailofbits/cargo-unmaintained/pull/451))

## 1.5.0

- Clarify "newer version is available" message ([#394](https://github.com/trailofbits/cargo-unmaintained/pull/394))
- FEATURE: Add `--save-token` option to store a personal access token in $HOME/.config/cargo-unmaintained/token.txt on Linux/macOS, or %LOCALAPPDATA%\cargo-unmaintained\token.txt on Windows. Note that the existing means for providing a personal access token (`GITHUB_TOKEN_PATH` and `GITHUB_TOKEN`) continue to work as before. ([9a529aa](https://github.com/trailofbits/cargo-unmaintained/commit/9a529aadbada51a543b4db94cef21efd2c3f5ffc))

## 1.4.0

- Fix three bugs introduced by [#325](https://github.com/trailofbits/cargo-unmaintained/pull/325):
  - Avoid divide by zero in `Progress::draw` ([ef24aa9](https://github.com/trailofbits/cargo-unmaintained/commit/ef24aa968b4618a3beefd7daa989ace0082a8180))
  - Write warnings on new lines ([5d31493](https://github.com/trailofbits/cargo-unmaintained/commit/5d314938f0372fa8a222211bb21f4773a0330508))
  - Don't assert in `Progress::finish` ([#331](https://github.com/trailofbits/cargo-unmaintained/pull/331))
- Update README.md ([#334](https://github.com/trailofbits/cargo-unmaintained/pull/334) and [#340](https://github.com/trailofbits/cargo-unmaintained/pull/340))
- Update `--no-cache` description ([#329](https://github.com/trailofbits/cargo-unmaintained/pull/329))
- FEATURE: Before reporting that a package is unmaintained, verify that its latest version would be considered unmaintained as well ([#339](https://github.com/trailofbits/cargo-unmaintained/pull/339))

## 1.3.0

- FEATURE: Better progress reporting ([#325](https://github.com/trailofbits/cargo-unmaintained/pull/325))

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
