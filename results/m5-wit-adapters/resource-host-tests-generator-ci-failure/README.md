Actual GitHub Actions foundation failure at resource HEAD1e4d0e04.

[Run46117317885, job140894013537](https://github.com/bobby/suss/actions/runs/46117317885/job/140894013537).

`cargo test --workspace --locked --lib --bins --examples -- --test-threads=2` failed with13 compile errors because two new Wasmtime execution units were included in the generator crate. The generator deliberately has no Wasmtime dependency. The host-only cfg fix retains execution in suss-compile and follows the existing host/generator split. This is a downloaded raw CI log, not a local before/after source capture.
