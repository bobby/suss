# Imported WIT resource handles

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

This initial route rejects repeated aliases of a resource provider and foreign
resource dependencies until provider context is represented throughout the
codec graph. Borrowed results, exported handles,
guest-defined resources, generalized async resources and automatic scoped
exception/cancellation cleanup retain separate adapters. Unsupported selected
paths are diagnosed before source macro effects. This is a bounded imported
resource path, not complete WIT lifetime interoperability.
