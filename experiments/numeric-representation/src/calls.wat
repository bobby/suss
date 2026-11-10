;; Calling-convention candidates for ADR-0004 (#359). Each kernel loops n
;; times doing acc = f(acc), where f increments an i31, reached in a
;; different way. The callee is read from a mutable global each iteration, as
;; a var's root is, so nothing hoists it out of the loop.
(module
  (rec
    (type $args (array (mut eqref)))
    ;; Today's universal entry: environment and argument array.
    (type $uni (func (param eqref (ref $args)) (result eqref)))
    ;; A per-arity entry: the closure itself, then the argument.
    (type $f1 (func (param (ref $clo) eqref) (result eqref)))
    (type $clo (struct
      (field $env eqref)
      (field $invoke (ref $uni))   ;; today's typed function reference
      (field $min i32)
      (field $max i32)
      (field $idx1 i32)            ;; candidate: index of the 1-arity entry
      (field $ref1 (ref $f1))      ;; candidate: typed per-arity reference
      (field $any1 funcref)))      ;; candidate: untyped reference
    ;; The proposal: a closure holds the table index of its arity-0 entry,
    ;; and its entry for k arguments is at base + k. Each function's closure
    ;; is a subtype holding its captures as fields.
    (type $pclo (sub (struct (field $base i32))))
    (type $pclo_x (sub final $pclo (struct (field $base i32) (field $step eqref))))
    (type $p1 (func (param (ref $pclo) eqref) (result eqref)))
    ;; The same with one final closure type, whose environment is a final
    ;; struct per function: every cast compares a type ID.
    (type $fclo (struct (field $base i32) (field $env eqref)))
    (type $fenv (struct (field $step eqref)))
    (type $q1 (func (param (ref $fclo) eqref) (result eqref))))

  ;; One table of funcref, and one typed as the arity's signature.
  (table $tbl 5 funcref)
  (elem (table $tbl) (i32.const 4) func $q_e1)
  (table $typed 1 (ref null $f1))
  (elem (table $tbl) (i32.const 0) func $e1)
  ;; The proposal's function: base 1, so its 1-argument entry is at 2.
  (elem (table $tbl) (i32.const 2) func $p_e1)
  (elem (table $typed) (i32.const 0) (ref null $f1) (ref.func $e1))
  (elem declare func $dispatch $e1)

  (global $g (mut eqref) (ref.null eq))
  (global $gp (mut eqref) (ref.null eq))
  (func (export "setup")
    (global.set $g (struct.new $clo (ref.null eq) (ref.func $dispatch)
      (i32.const 1) (i32.const 1) (i32.const 0) (ref.func $e1) (ref.func $e1)))
    (global.set $gp (struct.new $pclo_x (i32.const 1) (ref.i31 (i32.const 1))))
    (global.set $gq (struct.new $fclo (i32.const 3) (struct.new $fenv (ref.i31 (i32.const 1))))))
  (global $gq (mut eqref) (ref.null eq))
  (func $q_e1 (type $q1) (param $self (ref $fclo)) (param $x eqref) (result eqref)
    (ref.i31 (i32.add (i31.get_s (ref.cast (ref i31) (local.get $x)))
      (i31.get_s (ref.cast (ref i31) (struct.get $fenv $step
        (ref.cast (ref $fenv) (struct.get $fclo $env (local.get $self)))))))))

  ;; The proposal's entry: casts itself to its closure type for a capture.
  (func $p_e1 (type $p1) (param $self (ref $pclo)) (param $x eqref) (result eqref)
    (ref.i31 (i32.add (i31.get_s (ref.cast (ref i31) (local.get $x)))
      (i31.get_s (ref.cast (ref i31)
        (struct.get $pclo_x $step (ref.cast (ref $pclo_x) (local.get $self))))))))
  ;; The out-of-line path for any other callable: keywords, collections,
  ;; vars, IFn objects, non-callables.
  (func $call1_slow (param $f eqref) (param $x eqref) (result eqref) unreachable)

  (func $direct1 (param $x eqref) (result eqref)
    (ref.i31 (i32.add (i31.get_s (ref.cast (ref i31) (local.get $x))) (i32.const 1))))
  (func $e1 (type $f1) (param $self (ref $clo)) (param $x eqref) (result eqref)
    (ref.i31 (i32.add (i31.get_s (ref.cast (ref i31) (local.get $x))) (i32.const 1))))

  ;; Today: the closure's dispatcher checks the array's length and tail-calls
  ;; the method, which reads its argument from the array.
  (func $dispatch (type $uni) (param $env eqref) (param $a (ref $args)) (result eqref)
    (if (i32.ne (array.len (local.get $a)) (i32.const 1)) (then unreachable))
    (return_call $method (local.get $env) (local.get $a)))
  (func $method (param $env eqref) (param $a (ref $args)) (result eqref)
    (ref.i31 (i32.add
      (i31.get_s (ref.cast (ref i31) (array.get $args (local.get $a) (i32.const 0))))
      (i32.const 1))))
  ;; Today's invoke: closure and array type checks, the arity range, then a
  ;; tail call through the closure's typed function reference.
  (func $invoke (param $f eqref) (param $a eqref) (result eqref)
    (local $c (ref $clo))
    (local $n i32)
    (if (i32.eqz (ref.test (ref $clo) (local.get $f))) (then unreachable))
    (if (i32.eqz (ref.test (ref $args) (local.get $a))) (then unreachable))
    (local.set $c (ref.cast (ref $clo) (local.get $f)))
    (local.set $n (array.len (ref.cast (ref $args) (local.get $a))))
    (if (i32.or
          (i32.lt_u (local.get $n) (struct.get $clo $min (local.get $c)))
          (i32.and
            (i32.ne (struct.get $clo $max (local.get $c)) (i32.const -1))
            (i32.gt_u (local.get $n) (struct.get $clo $max (local.get $c)))))
      (then unreachable))
    (return_call_ref $uni
      (struct.get $clo $env (local.get $c))
      (ref.cast (ref $args) (local.get $a))
      (struct.get $clo $invoke (local.get $c))))

  (func $result (param $acc eqref) (result i64)
    (i64.extend_i32_s (i31.get_s (ref.cast (ref i31) (local.get $acc)))))

  ;; Lower bound: the increment inline, no call.
  (func (export "inline") (param $n i64) (result i64)
    (local $acc eqref)
    (local.set $acc (ref.i31 (i32.const 0)))
    (block $done (loop $l
      (br_if $done (i64.eqz (local.get $n)))
      (local.set $n (i64.sub (local.get $n) (i64.const 1)))
      (drop (global.get $g))
      (local.set $acc (ref.i31 (i32.add (i31.get_s (ref.cast (ref i31) (local.get $acc))) (i32.const 1))))
      (br $l)))
    (call $result (local.get $acc)))

  ;; A static call, as direct linking (M15-07) would emit.
  (func (export "direct") (param $n i64) (result i64)
    (local $acc eqref)
    (local.set $acc (ref.i31 (i32.const 0)))
    (block $done (loop $l
      (br_if $done (i64.eqz (local.get $n)))
      (local.set $n (i64.sub (local.get $n) (i64.const 1)))
      (drop (global.get $g))
      (local.set $acc (call $direct1 (local.get $acc)))
      (br $l)))
    (call $result (local.get $acc)))

  ;; call_indirect through a funcref table, by an index the closure holds.
  (func (export "indirect") (param $n i64) (result i64)
    (local $acc eqref) (local $c (ref null $clo))
    (local.set $acc (ref.i31 (i32.const 0)))
    (block $done (loop $l
      (br_if $done (i64.eqz (local.get $n)))
      (local.set $n (i64.sub (local.get $n) (i64.const 1)))
      (local.set $c (ref.cast (ref $clo) (global.get $g)))
      (local.set $acc (call_indirect $tbl (type $f1)
        (ref.as_non_null (local.get $c)) (local.get $acc) (struct.get $clo $idx1 (local.get $c))))
      (br $l)))
    (call $result (local.get $acc)))

  ;; call_indirect through a table typed as the signature: no signature check.
  (func (export "indirect_typed") (param $n i64) (result i64)
    (local $acc eqref) (local $c (ref null $clo))
    (local.set $acc (ref.i31 (i32.const 0)))
    (block $done (loop $l
      (br_if $done (i64.eqz (local.get $n)))
      (local.set $n (i64.sub (local.get $n) (i64.const 1)))
      (local.set $c (ref.cast (ref $clo) (global.get $g)))
      (local.set $acc (call_indirect $typed (type $f1)
        (ref.as_non_null (local.get $c)) (local.get $acc) (struct.get $clo $idx1 (local.get $c))))
      (br $l)))
    (call $result (local.get $acc)))

  ;; call_ref of a typed table element: table.get, then call_ref.
  (func (export "table_ref") (param $n i64) (result i64)
    (local $acc eqref) (local $c (ref null $clo))
    (local.set $acc (ref.i31 (i32.const 0)))
    (block $done (loop $l
      (br_if $done (i64.eqz (local.get $n)))
      (local.set $n (i64.sub (local.get $n) (i64.const 1)))
      (local.set $c (ref.cast (ref $clo) (global.get $g)))
      (local.set $acc (call_ref $f1
        (ref.as_non_null (local.get $c)) (local.get $acc)
        (table.get $typed (struct.get $clo $idx1 (local.get $c)))))
      (br $l)))
    (call $result (local.get $acc)))

  ;; call_ref of a typed function reference held in the closure struct.
  (func (export "field_typed") (param $n i64) (result i64)
    (local $acc eqref) (local $c (ref null $clo))
    (local.set $acc (ref.i31 (i32.const 0)))
    (block $done (loop $l
      (br_if $done (i64.eqz (local.get $n)))
      (local.set $n (i64.sub (local.get $n) (i64.const 1)))
      (local.set $c (ref.cast (ref $clo) (global.get $g)))
      (local.set $acc (call_ref $f1
        (ref.as_non_null (local.get $c)) (local.get $acc) (struct.get $clo $ref1 (local.get $c))))
      (br $l)))
    (call $result (local.get $acc)))

  ;; call_ref of an untyped funcref field, cast to the signature.
  (func (export "field_untyped") (param $n i64) (result i64)
    (local $acc eqref) (local $c (ref null $clo))
    (local.set $acc (ref.i31 (i32.const 0)))
    (block $done (loop $l
      (br_if $done (i64.eqz (local.get $n)))
      (local.set $n (i64.sub (local.get $n) (i64.const 1)))
      (local.set $c (ref.cast (ref $clo) (global.get $g)))
      (local.set $acc (call_ref $f1
        (ref.as_non_null (local.get $c)) (local.get $acc)
        (ref.cast (ref $f1) (struct.get $clo $any1 (local.get $c)))))
      (br $l)))
    (call $result (local.get $acc)))

  ;; The proposal's call site: a closure test that branches out of line, the
  ;; entry at base + 1, and call_indirect.
  (func (export "proposed") (param $n i64) (result i64)
    (local $acc eqref) (local $c (ref null $fclo))
    (local.set $acc (ref.i31 (i32.const 0)))
    (block $done (loop $l
      (br_if $done (i64.eqz (local.get $n)))
      (local.set $n (i64.sub (local.get $n) (i64.const 1)))
      (block $call (result eqref)
        (block $slow (result eqref)
          global.get $gq
          br_on_cast_fail $slow eqref (ref $fclo)
          local.set $c
          (call_indirect $tbl (type $q1)
            (ref.as_non_null (local.get $c)) (local.get $acc)
            (i32.add (struct.get $fclo $base (local.get $c)) (i32.const 1)))
          br $call)
        local.get $acc
        call $call1_slow)
      local.set $acc
      (br $l)))
    (call $result (local.get $acc)))

  ;; The same with a closure subtype per function: the call site's cast to
  ;; the non-final supertype.
  (func (export "proposed_subtype") (param $n i64) (result i64)
    (local $acc eqref) (local $c (ref null $pclo))
    (local.set $acc (ref.i31 (i32.const 0)))
    (block $done (loop $l
      (br_if $done (i64.eqz (local.get $n)))
      (local.set $n (i64.sub (local.get $n) (i64.const 1)))
      (block $call (result eqref)
        (block $slow (result eqref)
          global.get $gp
          br_on_cast_fail $slow eqref (ref $pclo)
          local.set $c
          (call_indirect $tbl (type $p1)
            (ref.as_non_null (local.get $c)) (local.get $acc)
            (i32.add (struct.get $pclo $base (local.get $c)) (i32.const 1)))
          br $call)
        local.get $acc
        call $call1_slow)
      local.set $acc
      (br $l)))
    (call $result (local.get $acc)))

  ;; Today's path: the call site's callable test, an argument array, invoke.
  (func (export "today") (param $n i64) (result i64)
    (local $acc eqref) (local $f eqref)
    (local.set $acc (ref.i31 (i32.const 0)))
    (block $done (loop $l
      (br_if $done (i64.eqz (local.get $n)))
      (local.set $n (i64.sub (local.get $n) (i64.const 1)))
      (local.set $f (global.get $g))
      (if (i32.eqz (ref.test (ref $clo) (local.get $f))) (then unreachable))
      (local.set $acc (call $invoke (local.get $f)
        (array.new_fixed $args 1 (local.get $acc))))
      (br $l)))
    (call $result (local.get $acc))))
