//! Canonical tail append preserves public Clojure values, lifetime and effects.
use suss_cli::portable_session::{Session, SessionError};

// Explicit test-only allowance for repeated construction/equality at trie boundaries.
fn sessions() -> [Session; 2] {
    [Session::new_repl().unwrap(), Session::new_macro().unwrap()].map(|mut session| {
        session.set_operation_fuel(500_000_000);
        session
    })
}

fn printed(session: &mut Session, source: &str, trusted: bool) -> String {
    let source = format!("(pr-str {source})");
    let value = if trusted {
        session.eval_privileged(&source)
    } else {
        session.eval(&source)
    }
    .unwrap_or_else(|error| panic!("{source}: {error}"));
    session.collect().unwrap();
    let units = session
        .inspect(&value, |mut store, value| {
            let array = value.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
            Ok(array
                .elems(&mut store)?
                .map(|unit| unit.unwrap_i32() as u16)
                .collect::<Vec<_>>())
        })
        .unwrap();
    String::from_utf16(&units).unwrap()
}

#[test]
fn native_tail_admission_declines_before_any_mutation_at_full_tails() {
    for mut session in sessions() {
        assert_eq!(
            printed(
                &mut session,
                r#"
          (mapv (fn [n]
            (let [before (vec (range n)) t (transient before)
                  result (suss.bootstrap/transient-vector-append t :added)
                  declined (suss.bootstrap/undefined? result)
                  after (persistent! t)]
              [n declined (or declined (identical? t result))
               (= after (if declined before (conj before :added)))]))
            [0 1 30 31 32 33 63 64 65 1023 1024 1055 1056 1057])"#,
                true
            ),
            "[[0 false true true] [1 false true true] [30 false true true] \
            [31 false true true] [32 true true true] [33 false true true] \
            [63 false true true] [64 true true true] [65 false true true] \
            [1023 false true true] [1024 true true true] [1055 false true true] \
            [1056 true true true] [1057 false true true]]"
        );
    }
}

#[test]
fn public_growth_preserves_snapshots_across_tail_and_root_boundaries() {
    for mut session in sessions() {
        assert_eq!(
            printed(
                &mut session,
                r#"
          (mapv (fn [n]
            (let [before (vec (range n))
                  after (persistent! (reduce conj! (transient before) (range n (+ n 70))))]
              [n (count after) (nth after n) (peek after)
               (= before (vec (range n))) (= after (vec (range (+ n 70))))]))
            [0 1 30 31 32 33 63 64 65 1023 1024 1055 1056 1057])"#,
                false
            ),
            "[[0 70 0 69 true true] [1 71 1 70 true true] [30 100 30 99 true true] \
            [31 101 31 100 true true] [32 102 32 101 true true] [33 103 33 102 true true] \
            [63 133 63 132 true true] [64 134 64 133 true true] [65 135 65 134 true true] \
            [1023 1093 1023 1092 true true] [1024 1094 1024 1093 true true] \
            [1055 1125 1055 1124 true true] [1056 1126 1056 1125 true true] \
            [1057 1127 1057 1126 true true]]"
        );
    }
}

#[test]
fn generic_receivers_and_invalidated_transients_keep_argument_effects() {
    for mut session in sessions() {
        session
            .eval(
                r#"
          (deftype Tap [events]
            ITransientCollection
            (-conj! [this x] (swap! events conj x) this)
            (-persistent! [_] @events))
          (deftype NilTap [events]
            ITransientCollection
            (-conj! [_ x] (swap! events conj x) nil)
            (-persistent! [_] @events))"#,
            )
            .unwrap();
        assert_eq!(
            printed(
                &mut session,
                r#"
          [(let [events (atom []) t (Tap. events)
                 result (conj! (do (swap! events conj :receiver) t)
                               (do (swap! events conj :value) 7))]
             [(identical? t result) (persistent! t)])
           (let [events (atom [])]
             [(nil? (conj! (NilTap. events) 9)) @events])
           (let [events (atom []) t (transient [1]) p (persistent! t)
                 result (try (conj! (do (swap! events conj :receiver) t)
                                    (do (swap! events conj :value) 9))
                             :missing-error (catch :default e :threw))]
             [result p @events])
           (let [events (atom [])
                 result (try (conj! (do (swap! events conj :receiver) nil)
                                    (do (swap! events conj :value) 9))
                             :missing-error (catch :default e :threw))]
             [result @events])]"#,
                false
            ),
            "[[true [:receiver :value 7]] [true [9]] [:threw [1] [:receiver :value]] \
            [:threw [:receiver :value]]]"
        );
    }
}

#[test]
fn appended_values_keep_aliases_and_closures_after_gc() {
    for mut session in sessions() {
        session
            .eval(
                r#"
          (def retained-tail-values
            (let [a (atom 1) f (fn [] (swap! a inc))
                  t (transient [])]
              (reduce conj! t [a f nil false :tag "text" 1.5 9223372036854775807])
              [a f (persistent! t)]))"#,
            )
            .unwrap();
        session.collect().unwrap();
        assert_eq!(
            printed(
                &mut session,
                r#"
          (let [[a f p] retained-tail-values]
            [(identical? a (nth p 0)) (identical? f (nth p 1))
             ((nth p 1)) @a (subvec p 2)])"#,
                false
            ),
            "[true true 2 2 [nil false :tag \"text\" 1.5 9223372036854775807]]"
        );
    }
}

#[test]
fn private_adapters_cannot_register_or_mutate_from_ordinary_source() {
    for mut session in sessions() {
        for source in [
            "(suss.bootstrap/transient-vector-types! nil nil nil)",
            "(suss.bootstrap/transient-vector-append nil 1)",
        ] {
            let Err(SessionError::Compile(error)) = session.eval(source) else {
                panic!("private adapter was not rejected: {source}");
            };
            assert_eq!(
                error.message,
                "Private transient vector adapters require compiled core"
            );
        }
        session.eval("(ns suss.core)").unwrap();
        session
            .eval("(suss.bootstrap/transient-vector-types! nil nil nil)")
            .unwrap();
        assert_eq!(
            printed(
                &mut session,
                r#"
          (let [t (transient [])]
            [(suss.bootstrap/undefined? (suss.bootstrap/transient-vector-append t :x))
             (persistent! t)])"#,
                false
            ),
            "[true []]"
        );
        session.eval("(ns user)").unwrap();
        assert!(
            session
                .eval_privileged(
                    r#"
          (let [t (transient []) r (.-root t)]
            (suss.bootstrap/transient-vector-types! r t nil))"#
                )
                .is_err()
        );
        session.collect().unwrap();
        assert_eq!(
            printed(
                &mut session,
                r#"
          (let [t (transient []) result (suss.bootstrap/transient-vector-append t :ok)]
            [(suss.bootstrap/undefined? result) (persistent! t)])"#,
                true
            ),
            "[false [:ok]]"
        );
    }
}

#[test]
fn unsupported_counts_and_tail_storage_decline_without_writes() {
    for mut session in sessions() {
        assert_eq!(
            printed(
                &mut session,
                r#"
          [(let [root (.-root (transient []))]
             (mapv (fn [n]
               (let [tail (object-array (repeat 32 :untouched))
                     t (TransientVector. n 5 root tail)
                     result (suss.bootstrap/transient-vector-append t :new)]
                 [(suss.bootstrap/undefined? result) (identical? n (.-cnt t))
                  (= (vec tail) (vec (repeat 32 :untouched)))]))
               [-1 1073741823 1073741824 1.0 nil]))
           (let [t (transient [7]) tail (int-array 32)
                 _ (set! (.-tail t) tail)
                 result (suss.bootstrap/transient-vector-append t 9)]
             [(suss.bootstrap/undefined? result) (.-cnt t)
              (= (vec tail) (vec (repeat 32 0)))])
           (let [t (transient [7]) tail (object-array [7])
                 _ (set! (.-tail t) tail)
                 result (suss.bootstrap/transient-vector-append t 9)
                 error (try (conj! t 9) :missing-error (catch :default e :threw))]
             [(suss.bootstrap/undefined? result) error (.-cnt t) (vec tail)])]"#,
                true
            ),
            "[[[true true true] [true true true] [true true true] [true true true] \
             [true true true]] [true 1 true] [true :threw 1 [7]]]"
        );
    }
}

#[test]
fn aot_vector_growth_uses_the_same_public_values() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("app.sus"),
        r#"
      (ns app)
      (defn report [n]
        (let [before (vec (range n))
              after (persistent! (reduce conj! (transient before) (range n (+ n 70))))]
          (pr-str [n (count after) (nth after n) (peek after)
                   (= before (vec (range n)))])))"#,
    )
    .unwrap();
    std::fs::write(
        root.path().join("app.wit"),
        "package test:append; world w { export report: func(n: u32) -> string; }",
    )
    .unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_suss"))
        .current_dir(root.path())
        .args([
            "compile",
            "app.sus",
            "-w",
            "app.wit",
            "-o",
            "app.wasm",
            "--export",
            "report=app/report",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    for n in [0_u32, 31, 32, 1024, 1056] {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_suss"))
            .current_dir(root.path())
            .args([
                "run",
                "app.wasm",
                "--invoke",
                "report",
                "--",
                &n.to_string(),
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            String::from_utf8(output.stdout).unwrap().trim(),
            format!("[{n} {} {n} {} true]", n + 70, n + 69)
        );
    }
}
