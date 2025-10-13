## 12 October 2025

Updated Plan
  └ ✔ Create functions module skeleton (constants, error, mod wiring).
    ✔ Implement minimal Functions service with callable stub using reqwest (or placeholder), register component, expose
      API.
    ✔ Add tests, link module in crate root, run fmt/test.

> Minimal Cloud Functions API ported.

  - Added src/functions with constants, error handling, and Functions service registered via the component system.
  - Implemented get_functions (with optional region) and https_callable returning a stubbed callable that round-trips
  serialized data—enough for dependent modules to integrate.
  - Exposed the module in the crate root and added a unit test demonstrating callable usage.

  Formatting/tests: cargo fmt, cargo test (all 75 tests pass). Future work should focus on real HTTPS callable
  invocation (auth, fetch), emulator support, and the richer helpers present in the JS SDK.