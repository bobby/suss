# Host WIT resource handles

Synchronous imported constructors, methods and statics use the same explicit
Runtime mappings as [ordinary WIT imports](wit-imports.md). Constructors return
opaque GC values. Methods borrow a live owner during the call; statics use their
declared ordinary arguments. `suss.resource/close!` releases an owned handle and
returns nil. Closing invalidates every alias of that handle.

```wit
package example:counters;
world api {
  import storage: interface {
    resource counter {
      constructor(value: u64);
      read: func() -> u64;
      echo: static func(value: u64) -> u64;
    }
  }
  export run: func(value: u64) -> u64;
}
```

```clojure
(ns app)
(defn run [value]
  (let [handle (host/open value)
        result (host/read handle)]
    (suss.resource/close! handle)
    (host/echo result)))
```

Map the resolved member paths, quoting bracketed names in a shell:

```sh
suss compile app.sus -w api.wit --wit-world api \
  --import 'storage#[constructor]counter=host/open' \
  --import 'storage#[method]counter.read=host/read' \
  --import 'storage#[static]counter.echo=host/echo' \
  --export run=app/run -o app.wasm
```

The embedding host supplies the declared resource type, its destructor and the
interface functions. The executing Rust fixture is
`crates/suss-compile/tests/portable_aot_wit_resources.rs`. A type declaration
alone does not supply a host storage implementation.

Handles retain a private declared-type check and ownership state. Numeric values
cannot stand in for them. Reuse after close or an ownership move raises a language boundary error
before a host call. GC keeps carrier state alive; finalization does not close
host resources. Keep ownership explicit. A canonical host trap requires Store
retirement as described in the import guide.

An imported `own<resource>` parameter transfers ownership to the host. Every
alias of the previous owner becomes unusable before the host is called. An owned
result creates a new owner, including when the host returns the same physical
resource. A typed `result::err` does not undo an ownership transfer.

All source arguments are projected and validated before canonical allocation.
The adapter also checks for conflicting aliases across the complete argument
set, including nested records, tuples, lists and option/result payloads. Passing
one owner twice, or both owning and borrowing that owner in one call, raises a
language boundary error and leaves the owner usable. Repeated borrows are valid.
A malformed later argument likewise leaves earlier owners unchanged. Canonical
buffers are written before the complete move is committed at handoff; collection
of individual arguments does not change their ownership.

Use `suss.resource/call-with-scope` for explicit lexical cleanup. It takes a
zero-arity function and returns that function's value:

```clojure
(suss.resource/call-with-scope
  (fn []
    (let [handle (host/open value)]
      (host/read handle))))
```

Every owned result created inside the scope is registered for cleanup, including
owners nested in returned data. On normal return or a language exception, live
owners close in reverse creation order. Already closed or moved owners are
skipped. Nested scopes restore the outer scope before running destructors.
Escaping an owner from its scope leaves a closed handle; subsequent operations
raise a boundary error. Owners created outside the scope keep their explicit
close/move lifetime. This API executes a synchronous body; it does not extend a
scope across asynchronous suspension or provide cancellation cleanup.

Synchronous exported functions can receive `borrow<resource>` values, including
borrows nested in records and other data. The resource keeps its declared
provider identity. A source borrow is valid through the entire call, including
result projection. It cannot be closed or passed to an `own` parameter. At call
end its token expires and the adapter releases the canonical borrow entries;
the host's owned resource remains alive. Keeping a borrow in an atom or closure
does not extend its lifetime. It stays expired even during a later call that
borrows the same host resource again.

Language exceptions run the adapter's cleanup before propagating. At the pinned
Wasmtime component boundary an uncaught exception becomes `UncaughtException`,
with its original pending exception available for inspection in the Store.
Inspect the error, then retire that Store. Ordinary WIT result errors and caught
language exceptions remain normal values/returns.

Synchronous exports also accept and return `own<resource>`, including nested
values. An incoming owner belongs to Suss until it is explicitly closed or
moved. It may be retained in an atom across calls and GC; returning from the
export does not implicitly close it. Use `try`/`finally` to close an incoming
owner after temporary use. The lexical scope API only registers owners created
inside its body.

Returning an owner transfers it to the host and invalidates every old Suss
alias, preserving the physical host resource identity. All result values and
ownership aliases are checked before canonical result allocation. Result moves
commit after any borrowed-input cleanup, immediately before return. A list
containing the same owner twice rejects before ownership is handed off.

An imported interface may declare only resource types, with no mapped host
functions. The host still supplies each declared type and its destructor.
Suss installs the lifetime operations and canonical drops without adding
component imports beyond the selected world.

This initial route rejects repeated aliases of a resource provider and foreign
resource dependencies until provider context is represented throughout the
codec graph. The pinned WIT resolver rejects borrowed results. Guest-defined
resources and generalized async/cancellation lifetimes retain separate adapters.
Unsupported selected paths are diagnosed before source macro effects. Full WIT
lifetime interoperability requires those remaining paths.

Canonical WIT map values cannot contain own/borrow handles, including nested
handles. The pinned validator forbids them because duplicate-key conversion
can discard a value. Suss diagnoses this selected schema before source macros.
Resource-bearing records, tuples and lists use their declared supported schemas.
