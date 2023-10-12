# cargo-unmaintained

Find unmaintained dependencies in Rust projects

This tool considers a dependency X unmaintained if-and-only-if X satisfies the following two criteria:

- X relies on a version of a dependency that is incompatible with the dependency's latest version.
- Either X has no associated repository, or its repository's last commit was over a year ago (a configurable value).

Notes:

1. By these criteria, a "leaf" dependency (i.e., a dependency with no dependencies) is never considered unmaintained.
2. Actively maintained dependencies tend to receive updates even when new versions of the dependency are not being published. Such updates might include improvements to documentation, addition of tests, etc. This is the reason for the second criterion.
