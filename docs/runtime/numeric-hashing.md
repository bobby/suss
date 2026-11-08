# Numeric equality and hashing

Numbers follow Clojure 1.12
([ADR-0003](../adr/0003-clojure-numeric-tower.md), #336). This page replaces
the binary64 notes that preceded that decision, including the former
`hash-numeric-boundaries` page.

## Equality

`=` compares numbers only within a category, as Clojure's `Util.equiv` does:

| Expression | Result |
| --- | --- |
| `(= 1 1.0)` | false |
| `(= 0.0 -0.0)` | true |
| `(= ##NaN ##NaN)` | false |
| `(= [1 2] [1 2.0])` | false |
| `(== 1 1.0)` | true |

Core's `=` tests scalars with the private `suss.bootstrap/equiv`, the runtime's
`number-equiv`. Two longs compare as 64-bit integers, two doubles by IEEE-754
equality, and a long never equals a double or a non-number. Other values fall
through to strict identity and then `-equiv`, as before. `==` compares numeric
value across categories and throws on a non-number.

## Hashing

`hash` of a number is Clojure's `hasheq`, the runtime's `number-hash`, reached
through the private `suss.bootstrap/hash-number`:

- A long hashes with Murmur3's `hashLong`: its low and high words mixed in
  turn, finalized with length 8. Zero hashes to 0.
- A double hashes as `Double.hashCode`, the XOR of its bits' two words, with
  NaN's canonical bits. -0.0 hashes as 0.0, to 0, as `Numbers.hasheq` makes it,
  so the two zeros that `=` equates hash alike.

| Value | `hash` |
| --- | --- |
| `1` | 1392991556 |
| `-1` | 1651860712 |
| `9223372036854775807` | -2106506049 |
| `1.0` | 1072693248 |
| `2.5` | 1074003968 |
| `##NaN` | 2146959360 |

A long and the equal double hash differently, as in Clojure, so hash maps and
sets keep them apart. Hash maps, and sets of more than eight elements, iterate
in the JVM's order because their hashes, and core's hash tries, match
Clojure's. A set of eight or fewer elements still iterates in insertion order,
as ClojureScript's array-backed small sets do; JVM Clojure orders it by hash.

## Core's 32-bit hashing

Shipped core's string, keyword, symbol and collection hashing is
ClojureScript's Murmur3 and hash-trie code, written for 32-bit bit operations.
Inside `suss.core`, inline `int`, `bit-and`, `bit-or`, `bit-xor`,
`bit-shift-left`, `bit-shift-right`, `unsigned-bit-shift-right` and `imul`
lower to private `int32-*` runtime helpers with ClojureScript's semantics, so
those algorithms and their results are unchanged. User code gets Clojure's
64-bit operations ([bit operations](bitwise-hashing.md)).
