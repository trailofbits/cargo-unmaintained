# cargo-unmaintained

**Find unmaintained packages in Rust projects**

`cargo-unmaintained` is similar to [`cargo-audit`]. However, `cargo-unmaintained` finds unmaintained packages automatically using heuristics, rather than rely on users to manually submit them to the [RustSec Advisory Database].

`cargo-unmaintained` defines an unmaintained package X as one that satisfies one of 1 through 3 below:

1. X's repository is archived (see [Notes] below).

2. X is not a member of its named repository.

3. Both a and b below.

   a. X depends on a package Y whose latest version:
   - is incompatible with the version that X depends on
   - was released over a year ago (a configurable value)

   b. Either X has no associated repository, or its repository's last commit was over a year ago (a configurable value).

<!-- as-of start -->

As of 2025-06-11, the RustSec Advisory Database contains 139 active advisories for unmaintained packages. Using the above conditions, `cargo-unmaintained` automatically identifies 104 (74%) of them. These results can be reproduced by running the [`rustsec_advisories`] example within this repository.

<!-- as-of end -->

### Notes

- To check whether packages' repositories have been archived, set the `GITHUB_TOKEN_PATH` environment variable to the path of a file containing a [personal access token]. If unset, this check will be skipped.

- The above conditions consider a "leaf" package (i.e., a package with no dependencies) unmaintained only if conditions 1 or 2 apply.

- The purpose of the "over a year ago" qualifications in condition 3 is to give package maintainers a chance to update their packages. That is, an incompatible upgrade to one of X's dependencies could require time-consuming changes to X. Without this check, `cargo-unmaintained` would produce many false positives.

<!-- not-identified start -->

- Of the 35 packages in the RustSec Advisory Database _not_ identified by `cargo-unmaintained`:
  - 11 do not build
  - 3 are existent, unarchived leaves
  - 3 were updated within the past 365 days
  - 18 were not identified for other reasons

<!-- not-identified end -->

## Output

`cargo-unmaintained`'s output includes the number of days since a package's repository was last updated, along with the dependencies that cause the package to be considered unmaintained.

For example, the following is the output produced by running `cargo-unmaintained` on [Cargo 0.74.0] on 2023-11-11:

<!--
`Scanning 357 packages and their dependencies (pass --verbose for more information)`
-->

<img src="etc/output.png" width=725>

## Installation

```sh
cargo install cargo-unmaintained
```

## Usage

```
Usage: cargo unmaintained [OPTIONS]

Options:
      --color <WHEN>    When to use color: always, auto, or never [default: auto]
      --fail-fast       Exit as soon as an unmaintained package is found
      --json            Output JSON (experimental)
      --max-age <DAYS>  Age in days that a repository's last commit must not exceed for the
                        repository to be considered current; 0 effectively disables this check,
                        though ages are still reported [default: 365]
      --no-cache        Do not cache data on disk for future runs
      --no-exit-code    Do not set exit status when unmaintained packages are found
      --no-warnings     Do not show warnings
  -p, --package <NAME>  Check only whether package NAME is unmaintained
      --purge           Remove all cached data from disk and exit
      --save-token      Read a personal access token from standard input and save it to
                        $HOME/.config/cargo-unmaintained/token.txt
      --tree            Show paths to unmaintained packages
      --verbose         Show information about what cargo-unmaintained is doing
  -h, --help            Print help
  -V, --version         Print version

The `GITHUB_TOKEN_PATH` environment variable can be set to the path of a file containing a personal
access token. If set, cargo-unmaintained will use this token to authenticate to GitHub and check
whether packages' repositories have been archived.

Alternatively, the `GITHUB_TOKEN` environment variable can be set to a personal access token.
However, use of `GITHUB_TOKEN_PATH` is recommended as it is less likely to leak the token.

If neither `GITHUB_TOKEN_PATH` nor `GITHUB_TOKEN` is set, but a file exists at
$HOME/.config/cargo-unmaintained/token.txt, cargo-unmaintained will use that file's contents as a
personal access token.

Unless --no-exit-code is passed, the exit status is 0 if no unmaintained packages were found and no
irrecoverable errors occurred, 1 if unmaintained packages were found, and 2 if an irrecoverable
error occurred.
```

## Ignoring packages

If a workspace's `Cargo.toml` file includes a `workspace.metadata.unmaintained.ignore` array, all packages named therein will be ignored. Example:

```toml
[workspace.metadata.unmaintained]
ignore = ["matchers"]
```

## Testing

Running just `cargo test` will not run the "continuous integration" or "externally influenced" tests. To run those additional tests, add `--workspace`, i.e.:

```sh
cargo test --workspace
```

## Known problems

- If a package is renamed from X to Y, it is immediately considered unmaintained because the package's repository no longer contains a package named X. ([#441])

  <details>

  <summary>Discussion</summary>

  I (@smoelius) suspect there may be no good solution to this problem.

  PRs [#575] and [#613] explored the possibility of finding a package Y with the same properties package X but a different name. However, arbitrary changes could be made to a package before its name is changed. This fact complicates such package matching.

  The `toml_write` package provides an example. Version 0.1.2 was published at commit [838a022] (2025-06-06). With commit [8658e70] (2025-06-12), the keyword `no_std` was added to its `Cargo.toml` file. Finally, with commit [b3594df] (2025-07-07), `toml_write` was renamed to `toml_writer`. Thus, the published `toml_write` package does not match the repository's `toml_writer` package because the latter's keywords include `no_std`.

  </details>

- If a project relies on an old version of a package, `cargo-unmaintained` may fail to flag the package as unmaintained (i.e., may produce a false negative). The following is a sketch of how this can occur.
  - The project relies on version 1 of package X, which has no dependencies.
  - Version 2 of package X exists, and adds version 1 of package Y as a dependency.
  - Version 2 of package Y exists.

  Note that version 1 of package X appears maintained, but version 2 does not. Ignoring a few details, version 2 satisfies condition 3 above.

  `cargo-unmaintained` does not, in all cases, check whether the latest version of a package is used, as doing so would be cost prohibitive. A downside of this choice is that false negatives can result.

  Note that false _positives_ should not arise in a corresponding way. Before flagging a package as unmaintained, `cargo-unmaintained` verifies that the package's latest version would be considered unmaintained as well.

## Questions

- Yesterday, I got a warning about an unmaintained package. But, today, I don't. Why is that?

  Possibly, an intermediate dependency was updated. Suppose package X depends on Y, which depends on Z. And suppose Z is considered unmaintained. Then Z will generate warnings for both X and Y. If Y is updated to no longer depend upon Z, and X uses the new version of Y, then X will no longer receive warnings about Z.

## Anti-goals

`cargo-unmaintained` is not meant to be a replacement for [`cargo-upgrade`]. `cargo-unmaintained` should not warn just because a package needs to be upgraded.

## Semantic versioning policy

We reserve the right to change the following and to consider such changes non-breaking:

- what data is stored in the cache, as well as how that data is stored
- the output produced by the experimental `--json` option

## License

`cargo-unmaintained` is licensed and distributed under the AGPLv3 license. [Contact us](mailto:opensource@trailofbits.com) if you're looking for an exception to the terms.

[#441]: https://github.com/trailofbits/cargo-unmaintained/issues/441
[#575]: https://github.com/trailofbits/cargo-unmaintained/pull/575
[#613]: https://github.com/trailofbits/cargo-unmaintained/pull/613
[838a022]: https://github.com/toml-rs/toml/commit/838a0223142a2137b530e020cb7231aba46f7946
[8658e70]: https://github.com/toml-rs/toml/commit/8658e70adde19e1f55edd6b1f8e8d33fe5ee6151
[Cargo 0.74.0]: https://github.com/rust-lang/cargo/tree/d252bce6553c8cc521840c9dd6b9f6cd4aedd8b0
[Notes]: #notes
[RustSec Advisory Database]: https://github.com/RustSec/advisory-db/
[`cargo-audit`]: https://github.com/RustSec/rustsec/tree/main/cargo-audit
[`cargo-upgrade`]: https://github.com/killercup/cargo-edit?tab=readme-ov-file#cargo-upgrade
[`rustsec_advisories`]: ./examples/rustsec_advisories.rs
[b3594df]: https://github.com/toml-rs/toml/commit/b3594df3b76a95d5d21f5af3a9847e44917c640a
[personal access token]: https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/managing-your-personal-access-tokens
