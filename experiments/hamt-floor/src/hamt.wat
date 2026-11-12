;; A Clojure-style persistent hash map (HAMT) written directly in WasmGC, to
;; find the lowest cost of `get` and `assoc` that wasmtime gives this design.
;;
;; This is a template. src/main.rs instantiates the block between the VARIANT
;; markers once per variant, replacing:
;;   @S                the variant's suffix (typed or dispatch);
;;   @HASH(x)          a hash of key x;
;;   @EQUIV(a, b)      key equality, an i32 truth value;
;;   @GET(m, k)        the map lookup the kernel calls;
;;   @ASSOC(m, k, v)   the map update the kernel calls.
;;
;; The node layout follows clojure.lang.PersistentHashMap's
;; BitmapIndexedNode: a 32-bit bitmap and an array of key/value pairs, where a
;; null key means that the value slot holds a child node. Clojure switches a
;; node with 16 or more children to an ArrayNode; this map doesn't, so its root
;; costs a popcount that Clojure's doesn't. Full-hash collisions trap; keys
;; 0..1023 have none.
;;
;; Keys and values are i31 refs, as Suss's small longs are, and are held as
;; eqref, as Suss holds every value. The hash is Clojure's Murmur3.hashLong.
(module
  (type $arr (array (mut eqref)))
  (type $node (struct (field $bitmap i32) (field $array (ref $arr))))
  (type $hashfn (func (param eqref) (result i32)))
  (type $equivfn (func (param eqref eqref) (result i32)))
  (type $getfn (func (param eqref eqref) (result eqref)))
  (type $assocfn (func (param eqref eqref eqref) (result eqref)))

  ;; The map the kernels read, like a Suss var. Mutable, so it can't be folded.
  (global $map (mut eqref) (ref.null eq))
  ;; Where assoc kernels store their result, so the update can't be dropped.
  (global $sink (mut eqref) (ref.null eq))
  (global $mask (mut i32) (i32.const 1023))

  ;; Function references for the dispatch variant. Immutable globals, but
  ;; wasmtime 49 still calls them indirectly, as it does a Suss closure.
  (global $hash_ref (ref $hashfn) (ref.func $hash_direct))
  (global $equiv_ref (ref $equivfn) (ref.func $equiv_direct))
  (global $get_ref (ref $getfn) (ref.func $get_dispatch_impl))
  (global $assoc_ref (ref $assocfn) (ref.func $assoc_dispatch_impl))

  ;; Murmur3.hashLong of an i31 key, written out in one function.
  (func $hash_direct (param $key eqref) (result i32)
    (local $v i32) (local $k i32) (local $h i32)
    (local.set $v (i31.get_s (ref.cast (ref i31) (local.get $key))))
    (if (i32.eqz (local.get $v)) (then (return (i32.const 0))))
    ;; low word
    (local.set $k (i32.mul (i32.rotl (i32.mul (local.get $v) (i32.const 0xcc9e2d51)) (i32.const 15)) (i32.const 0x1b873593)))
    (local.set $h (i32.add (i32.mul (i32.rotl (local.get $k) (i32.const 13)) (i32.const 5)) (i32.const 0xe6546b64)))
    ;; high word: the sign extension of the low word
    (local.set $k (i32.mul (i32.rotl (i32.mul (i32.shr_s (local.get $v) (i32.const 31)) (i32.const 0xcc9e2d51)) (i32.const 15)) (i32.const 0x1b873593)))
    (local.set $h (i32.add (i32.mul (i32.rotl (i32.xor (local.get $h) (local.get $k)) (i32.const 13)) (i32.const 5)) (i32.const 0xe6546b64)))
    ;; fmix with length 8
    (local.set $h (i32.xor (local.get $h) (i32.const 8)))
    (local.set $h (i32.xor (local.get $h) (i32.shr_u (local.get $h) (i32.const 16))))
    (local.set $h (i32.mul (local.get $h) (i32.const 0x85ebca6b)))
    (local.set $h (i32.xor (local.get $h) (i32.shr_u (local.get $h) (i32.const 13))))
    (local.set $h (i32.mul (local.get $h) (i32.const 0xc2b2ae35)))
    (i32.xor (local.get $h) (i32.shr_u (local.get $h) (i32.const 16))))

  ;; Equality of i31 keys is identity.
  (func $equiv_direct (param $a eqref) (param $b eqref) (result i32)
    (ref.eq (local.get $a) (local.get $b)))

  (func $clone_set (param $a (ref $arr)) (param $i i32) (param $x eqref) (result (ref $arr))
    (local $new (ref $arr))
    (local.set $new (array.new_default $arr (array.len (local.get $a))))
    (array.copy $arr $arr (local.get $new) (i32.const 0) (local.get $a) (i32.const 0) (array.len (local.get $a)))
    (array.set $arr (local.get $new) (local.get $i) (local.get $x))
    (local.get $new))

  (func $empty (result (ref $node))
    (struct.new $node (i32.const 0) (array.new_fixed $arr 0)))

  ;; VARIANT BEGIN
  (func $find_@S (param $n (ref $node)) (param $hash i32) (param $key eqref) (result eqref)
    (local $shift i32) (local $bm i32) (local $bit i32) (local $idx i32)
    (local $a (ref $arr)) (local $k eqref)
    (loop $walk
      (local.set $bm (struct.get $node $bitmap (local.get $n)))
      (local.set $bit (i32.shl (i32.const 1)
        (i32.and (i32.shr_u (local.get $hash) (local.get $shift)) (i32.const 31))))
      (if (i32.eqz (i32.and (local.get $bm) (local.get $bit)))
        (then (return (ref.null eq))))
      (local.set $idx (i32.shl (i32.popcnt (i32.and (local.get $bm)
        (i32.sub (local.get $bit) (i32.const 1)))) (i32.const 1)))
      (local.set $a (struct.get $node $array (local.get $n)))
      (local.set $k (array.get $arr (local.get $a) (local.get $idx)))
      (if (ref.is_null (local.get $k))
        (then
          (local.set $n (ref.cast (ref $node)
            (array.get $arr (local.get $a) (i32.add (local.get $idx) (i32.const 1)))))
          (local.set $shift (i32.add (local.get $shift) (i32.const 5)))
          (br $walk)))
      (if @EQUIV((local.get $key), (local.get $k))
        (then (return (array.get $arr (local.get $a) (i32.add (local.get $idx) (i32.const 1))))))
      (return (ref.null eq)))
    (unreachable))

  (func $create_@S (param $shift i32) (param $k1 eqref) (param $v1 eqref)
                   (param $h2 i32) (param $k2 eqref) (param $v2 eqref) (result (ref $node))
    (local $h1 i32)
    (local.set $h1 @HASH((local.get $k1)))
    (if (i32.eq (local.get $h1) (local.get $h2)) (then (unreachable)))
    (call $assoc_node_@S
      (call $assoc_node_@S (call $empty) (local.get $shift) (local.get $h1) (local.get $k1) (local.get $v1))
      (local.get $shift) (local.get $h2) (local.get $k2) (local.get $v2)))

  (func $assoc_node_@S (param $n (ref $node)) (param $shift i32) (param $hash i32)
                       (param $key eqref) (param $val eqref) (result (ref $node))
    (local $bm i32) (local $bit i32) (local $idx i32) (local $a (ref $arr))
    (local $k eqref) (local $v eqref) (local $sub (ref $node)) (local $new (ref $arr)) (local $len i32)
    (local.set $bm (struct.get $node $bitmap (local.get $n)))
    (local.set $bit (i32.shl (i32.const 1)
      (i32.and (i32.shr_u (local.get $hash) (local.get $shift)) (i32.const 31))))
    (local.set $idx (i32.shl (i32.popcnt (i32.and (local.get $bm)
      (i32.sub (local.get $bit) (i32.const 1)))) (i32.const 1)))
    (local.set $a (struct.get $node $array (local.get $n)))
    (if (i32.and (local.get $bm) (local.get $bit))
      (then
        (local.set $k (array.get $arr (local.get $a) (local.get $idx)))
        (local.set $v (array.get $arr (local.get $a) (i32.add (local.get $idx) (i32.const 1))))
        (if (ref.is_null (local.get $k))
          (then
            (local.set $sub (call $assoc_node_@S (ref.cast (ref $node) (local.get $v))
              (i32.add (local.get $shift) (i32.const 5)) (local.get $hash) (local.get $key) (local.get $val)))
            (if (ref.eq (local.get $sub) (local.get $v)) (then (return (local.get $n))))
            (return (struct.new $node (local.get $bm)
              (call $clone_set (local.get $a) (i32.add (local.get $idx) (i32.const 1)) (local.get $sub))))))
        (if @EQUIV((local.get $key), (local.get $k))
          (then
            (if (ref.eq (local.get $val) (local.get $v)) (then (return (local.get $n))))
            (return (struct.new $node (local.get $bm)
              (call $clone_set (local.get $a) (i32.add (local.get $idx) (i32.const 1)) (local.get $val))))))
        ;; Two keys share this slot: push both into a child node.
        (local.set $sub (call $create_@S (i32.add (local.get $shift) (i32.const 5))
          (local.get $k) (local.get $v) (local.get $hash) (local.get $key) (local.get $val)))
        (local.set $new (call $clone_set (local.get $a) (local.get $idx) (ref.null eq)))
        (array.set $arr (local.get $new) (i32.add (local.get $idx) (i32.const 1)) (local.get $sub))
        (return (struct.new $node (local.get $bm) (local.get $new)))))
    ;; Insert a new pair at idx.
    (local.set $len (array.len (local.get $a)))
    (local.set $new (array.new_default $arr (i32.add (local.get $len) (i32.const 2))))
    (array.copy $arr $arr (local.get $new) (i32.const 0) (local.get $a) (i32.const 0) (local.get $idx))
    (array.set $arr (local.get $new) (local.get $idx) (local.get $key))
    (array.set $arr (local.get $new) (i32.add (local.get $idx) (i32.const 1)) (local.get $val))
    (array.copy $arr $arr (local.get $new) (i32.add (local.get $idx) (i32.const 2))
      (local.get $a) (local.get $idx) (i32.sub (local.get $len) (local.get $idx)))
    (struct.new $node (i32.or (local.get $bm) (local.get $bit)) (local.get $new)))

  (func $get_@S_impl (type $getfn) (param $m eqref) (param $key eqref) (result eqref)
    (call $find_@S (ref.cast (ref $node) (local.get $m)) @HASH((local.get $key)) (local.get $key)))

  (func $assoc_@S_impl (type $assocfn) (param $m eqref) (param $key eqref) (param $val eqref) (result eqref)
    (call $assoc_node_@S (ref.cast (ref $node) (local.get $m)) (i32.const 0)
      @HASH((local.get $key)) (local.get $key) (local.get $val)))

  ;; (get m (bit-and i 1023)), n times; returns how many keys were found.
  (func (export "get_@S") (param $n i32) (result i32)
    (local $i i32) (local $c i32)
    (block $done
      (loop $next
        (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
        (if (i32.eqz (ref.is_null @GET((global.get $map),
              (ref.i31 (i32.and (local.get $i) (global.get $mask))))))
          (then (local.set $c (i32.add (local.get $c) (i32.const 1)))))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $next)))
    (local.get $c))

  ;; (assoc m (bit-and i 1023) i), n times, on the same map; returns n.
  (func (export "assoc_@S") (param $n i32) (result i32)
    (local $i i32)
    (block $done
      (loop $next
        (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
        (global.set $sink @ASSOC((global.get $map),
          (ref.i31 (i32.and (local.get $i) (global.get $mask))), (ref.i31 (local.get $i))))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $next)))
    (local.get $n))
  ;; VARIANT END

  ;; The kernels' loop alone: build the key and test a result, n times.
  (func (export "control") (param $n i32) (result i32)
    (local $i i32) (local $c i32)
    (block $done
      (loop $next
        (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
        (global.set $sink (ref.i31 (i32.and (local.get $i) (global.get $mask))))
        (if (i32.eqz (ref.is_null (global.get $sink)))
          (then (local.set $c (i32.add (local.get $c) (i32.const 1)))))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $next)))
    (local.get $c))

  ;; The value at key k in the last assoc kernel's result, or -1.
  (func (export "sink_get") (param $k i32) (result i32)
    (local $v eqref)
    (local.set $v (call $get_typed_impl (global.get $sink) (ref.i31 (local.get $k))))
    (if (result i32) (ref.is_null (local.get $v))
      (then (i32.const -1))
      (else (i31.get_s (ref.cast (ref i31) (local.get $v))))))

  ;; Builds (zipmap (range count) (range count)) and checks every key.
  (func (export "setup") (param $count i32) (result i32)
    (local $i i32) (local $m eqref) (local $found i32)
    (local.set $m (call $empty))
    (block $done
      (loop $next
        (br_if $done (i32.ge_u (local.get $i) (local.get $count)))
        (local.set $m (call $assoc_typed_impl (local.get $m)
          (ref.i31 (local.get $i)) (ref.i31 (local.get $i))))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $next)))
    (global.set $map (local.get $m))
    (local.set $i (i32.const 0))
    (block $done
      (loop $next
        (br_if $done (i32.ge_u (local.get $i) (local.get $count)))
        (if (ref.eq (call $get_typed_impl (local.get $m) (ref.i31 (local.get $i)))
                    (ref.i31 (local.get $i)))
          (then (local.set $found (i32.add (local.get $found) (i32.const 1)))))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $next)))
    (local.get $found))
)
