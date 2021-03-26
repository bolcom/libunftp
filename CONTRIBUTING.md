# Contributing to libunftp

libunftp welcomes contribution from everyone in the form of suggestions, bug reports, pull requests, and feedback.

Please reach out here in a GitHub issue if we can do anything to help you contribute.

# Contributing to the bundled authentication or storage back-end implementations

Please find them in the crates directory. We welcome improvements to them.

# Developing your own authentication or storage back-end implementation

We would love to see many of these emerge on crates.io to create an ecosystem of usable FTP building blocks for Rust. If
we can ask that you prefix your crate names:

- unftp-auth-* for authentication implementations
- unftp-sbe-* for storage back-end implementations

Keeping this naming convention will allow a consistent and easy way for people to find libunftp extentions on crates.io

## Submitting bug reports and feature requests

When reporting a bug or asking for help, please include enough details so that the people helping you can reproduce the behavior you are seeing. For some tips on how to approach this, read about how to produce a [Minimal, Complete, and Verifiable example](https://stackoverflow.com/help/mcve).

When making a feature request, please make it clear what problem you intend to solve with the feature, any ideas for how libunftp could support solving that problem, any possible alternatives, and any disadvantages.

## Checking your code

We encourage you to check that the test suite passes locally and make sure that clippy and rustfmt are happy before 
submitting a pull request with your changes. If anything does not pass, typically it will be easier to iterate and 
fix it locally than waiting for the CI servers to run tests for you. Pull requests that do not pass the CI pipeline 
will not be merged.

For your convenience we've added a makefile target. Simply run `make pr-prep` before your pull request.