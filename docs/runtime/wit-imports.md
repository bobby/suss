# Explicit WIT host functions

A selected WIT world can expose synchronous world and interface functions to
qualified Runtime vars. For example:

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
future and stream lifetimes need their own adapters. The current route accepts synchronous freestanding world and interface
functions. Mixed automatic randomness, optimization and generalized async
imports have separate integration work.

`portable_aot_wit_imports` and `compiled_aot_host_imports` retain executing
controls for numeric/Unicode fidelity, nested and nominal values, ownership,
Macro isolation, initialization, configured providers and cache invalidation.
Their separate command receipts and source qualifications are retained under
`results/m5-wit-adapters/`.

Interface functions use `PUBLIC-INTERFACE-PATH#FUNCTION`, for example
`operations#touch` for `import operations: data`, or
`test:service/data@1.2.3#touch` for the package-qualified provider. Every function
in the resolved selected world needs an explicit mapping. A `use` declaration
can add its original interface provider as a dependency alongside a named alias;
map both resolved providers when they expose functions. Imported instances keep
their declared names, versions, `implements` annotations and exported data-type
identities. Shared nominal dependencies alias the actual earlier provider types
in upstream's topological import order. Unsupported resource declarations reject
before canonical value layout or source macro execution.

`portable_aot_wit_interface_imports` executes imported and exported instances,
two aliases of one interface, shared nominal dependencies, native record/flag
updates, and a non-suspending canonical async export calling a synchronous
provider. It separately checks authority and unsupported selected types before
source macro effects. General async suspension and resource lifetimes retain
their own acceptance gates.


Interfaces containing only supported data-type declarations carry schema
identity without granting function authority. They need no imported-function
mapping or runtime linker provider; the component still declares the original
schema instance. Their complete selected types are checked before source macro
effects. The executing controls include a native record/list update with exact
integer extrema and a schema-only instance beside the automatic randomness
capability. Ordinary function-bearing interfaces still require explicit mappings.

External IDs on supported world functions, interface instances and interface
members survive into component runtime metadata. Hosts may inspect that metadata
to choose providers or exports. Source mappings still use the declared public
WIT paths; an external ID supplies no additional call authority. The same name
encoder retains `implements` and external-ID annotations for the automatic
randomness instance. The focused executing controls select imports/exports by ID
and exchange native collections with exact integers and Unicode.


[Imported resource handles](wit-resources.md) have a bounded synchronous
constructor/method/static route with opaque carriers and explicit close. Its
selection guard retains unsupported alias, move, borrow-result and export paths
until their lifetime adapters are implemented. Data-value mappings and ordinary
source collection operations remain unchanged.
