# Strings in indexed sequences

The pinned `IndexedSeq` reads an array or a string through `aget` and `alength`, as ClojureScript's `aget` reads a string's UTF-16 units. Suss's `aget`, `aset`, `alength` and `aclone` read only arrays, as on the JVM, and throw for a string (#425).

Core therefore reads an `IndexedSeq`'s backing, and `array-reduce`'s, through two private helpers:

- `indexed-get` returns a string's one-unit substring at an index, or an array's element;
- `indexed-length` returns a string's UTF-16 length, or an array's length.

`(seq "😀")` is two one-unit strings: the lone surrogates `"\uD83D"` and `"\uDE00"`.

The fast paths of `list`, `vector` and `set` take an `IndexedSeq`'s backing directly only when it is an object array. A string's sequence takes the general path.

`crates/suss-cli/tests/portable_indexed_strings.rs` checks that the array functions reject strings. `portable_iteration.rs`, `portable_sequences.rs` and `portable_reduction.rs` cover string sequences.
