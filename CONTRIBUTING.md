# Contributing to libunftp

libunft welcomes contribution from everyone in the form of suggestions, bug reports, pull requests, and feedback.

Please reach out here in a GitHub issue if we can do anything to help you contribute.

## Submitting bug reports and feature requests

When reporting a bug or asking for help, please include enough details so that the people helping you can reproduce the behavior you are seeing. For some tips on how to approach this, read about how to produce a [Minimal, Complete, and Verifiable example](https://stackoverflow.com/help/mcve).

When making a feature request, please make it clear what problem you intend to solve with the feature, any ideas for how libunftp could support solving that problem, any possible alternatives, and any disadvantages.

## Checking your code

We encourage you to check that the test suite passes locally and make sure that clippy and rustfmt are happy before submitting a pull request with your changes. If anything does not pass, typically it will be easier to iterate and fix it locally than waiting for the CI servers to run tests for you. Pull requests that do not pass the CI pipeline will not be merged.

##### In the project root

```sh
# Run all tests
cargo test --all-features
cargo clippy --all-features
cargo rustfmt
```

## License

By contributing your code, you agree to license your contribution under the terms of the [Apache License v2.0](http://www.apache.org/licenses/LICENSE-2.0). Your contributions should also include the following header:

```
/**
 * Copyright 2019 bol.com.
 * 
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 * 
 * http://www.apache.org/licenses/LICENSE-2.0
 * 
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */
 ```
