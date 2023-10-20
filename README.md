# cargo-unmaintained

Find unmaintained dependencies in Rust projects

This tool considers a dependency X unmaintained if-and-only-if X satisfies the following two criteria:

- X relies on a version of a dependency that is incompatible with the dependency's latest version.
- Either X has no associated repository, or its repository's last commit was over a year ago (a configurable value).

Notes:

1. By these criteria, a "leaf" dependency (i.e., a dependency with no dependencies) is never considered unmaintained.
2. Actively maintained dependencies tend to receive updates even when new versions of the dependency are not being published. Such updates might include improvements to documentation, addition of tests, etc. This is the reason for the second criterion.

## Usage

```
Usage: cargo unmaintained [OPTIONS]

Options:
      --tree            Show paths to unmaintained dependencies
      --no-exit-code    Do not set exit status when unmaintained dependencies are found
      --no-warnings     Do not show warnings
      --max-age <DAYS>  Age in days that a repository's last commit must not exceed for the
                        repository to be considered current; 0 effectively disables this check,
                        though ages are still reported [default: 365]
      --verbose         Show information about what cargo-unmaintained is doing
  -h, --help            Print help
  -V, --version         Print version

The `GITHUB_TOKEN` environment variable can be set to the path of a file containing a personal
access token, which will be used to authenticate to GitHub.

Unless --no-exit-code is passed, the exit status is 0 if-and-only-if no unmaintained dependencies
were found and no irrecoverable errors occurred.
```
