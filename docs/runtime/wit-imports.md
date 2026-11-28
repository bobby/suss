# Explicit WIT host functions

A selected WIT world can expose synchronous world functions to qualified
Runtime vars. For example:

```wit
package example:counter;
world api {
  import increment: func(value: s64) -> s64;
  export run: func(value: s64) -> s64;
}
```

```clojure
(ns app)
(defn run [value] (host/increment value))
```

Compile a file with explicit mappings:

```sh
suss compile src/app.sus -w api.wit --wit-world api \
  --import increment=host/increment --export run=app/run -o app.wasm
```

Namespace compilation accepts the same flags. A project can declare the
mappings in `deps.sus`:

```clojure
{:src-paths ["src"]
 :worlds
 {:app {:namespace app
        :wit "api.wit"
        :wit-world "api"
        :imports {"increment" host/increment}
        :exports {"run" app/run}
        :output "app.wasm"}}}
```

The host program supplies the declared WIT functions when it instantiates the
component. Mapping changes participate in CLI cache keys. A mapped namespace
cannot hide a source module and its initialization. Macro code receives no host
provider. Runtime import functions are installed before user initialization, so
initializers may call them and ordinary source calls keep using the same vars.

Arguments and results use the [native WIT value mappings](wit-values.md).
Source functions can use normal collection operations on returned values.
Every argument is projected and validated before transfer allocation. Source
argument effects retain their normal left-to-right order when a boundary error
is caught. Imported results are copied into guest GC storage before argument
and result transfer buffers are released. Signatures above the canonical flat
limit use upstream indirect records; imported multi-flat results receive a
return-pointer parameter.

A typed WIT `result::err` is an ordinary value. A canonical host trap poisons the
instance and requires Store retirement. Resource handles and error-context,
future and stream lifetimes need their own adapters. The current route accepts
synchronous world functions; imported interfaces, mixed automatic randomness,
optimization and generalized async imports have separate integration work.

`portable_aot_wit_imports` and `compiled_aot_host_imports` retain executing
controls for numeric/Unicode fidelity, nested and nominal values, ownership,
Macro isolation, initialization, configured providers and cache invalidation.
Their separate command receipts and source qualifications are retained under
`results/m5-wit-adapters/`.
