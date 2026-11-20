//! Checked canonical transient-map storage preserves public values and effects.
use suss_cli::portable_session::{Session, SessionError};

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
fn public_builds_updates_and_snapshots_keep_all_hash_trie_boundaries() {
    for mut session in sessions() {
        assert_eq!(
            printed(
                &mut session,
                r#"
          (mapv (fn [n]
            (let [before (into {} (map (fn [x] [x x])) (range n))
                  t (transient before)
                  edited (reduce (fn [t x] (assoc! t x (- x))) t (range n (+ n 70)))
                  after (persistent! edited)]
              [n (= before (into {} (map (fn [x] [x x])) (range n)))
               (= after (into before (map (fn [x] [x (- x)])) (range n (+ n 70))))
               (= after (persistent! (reduce conj! (transient before)
                    (mapv (fn [x] [x (- x)]) (range n (+ n 70))))))]))
            [0 1 7 8 15 16 31 32 63 64 255 256 1023 1024])"#,
                false
            ),
            "[[0 true true true] [1 true true true] [7 true true true] [8 true true true] \
            [15 true true true] [16 true true true] [31 true true true] [32 true true true] \
            [63 true true true] [64 true true true] [255 true true true] [256 true true true] \
            [1023 true true true] [1024 true true true]]"
        );
    }
}

#[test]
fn native_leaf_insert_and_update_are_atomic_and_keep_edit_lifetime() {
    for mut session in sessions() {
        assert_eq!(
            printed(
                &mut session,
                r#"
          (let [e (Unique.) arr (object-array (repeat 8 nil))
                node (BitmapIndexedNode. e 0 arr)
                t (TransientHashMap. e node 0 false nil)
                added (suss.bootstrap/transient-map-assoc t 1 :first)
                updated (suss.bootstrap/transient-map-assoc t 1 :second)
                p (persistent! t)
                after (suss.bootstrap/transient-map-assoc t 1 :third)]
            [(identical? t added) (identical? t updated) (= p {1 :second})
             (suss.bootstrap/undefined? after) (= p {1 :second})])"#,
                true
            ),
            "[true true true true true]"
        );
    }
}

#[test]
fn fallback_keys_values_gc_and_argument_effects_match_clojure() {
    for mut session in sessions() {
        session
            .eval(
                r#"
          (def retained-map-values
            (let [a (atom 1) f (fn [] (swap! a inc))
                  t (transient (into {} (map (fn [x] [x x])) (range 64)))]
              (conj! t [1 a]) (assoc! t 2 f)
              (assoc! t nil false :tag nil "text" 1.5 9223372036854775807 :wide)
              [a f (persistent! t)]))"#,
            )
            .unwrap();
        session.collect().unwrap();
        assert_eq!(
            printed(
                &mut session,
                r#"
          (let [[a f p] retained-map-values events (atom []) t (transient p)
                _ (persistent! t)
                failed (try (assoc! (do (swap! events conj :receiver) t)
                                   (do (swap! events conj :key) 9)
                                   (do (swap! events conj :value) :bad))
                            :missing (catch :default e :threw))]
            [(identical? a (get p 1)) (identical? f (get p 2))
             ((get p 2)) @a (get p nil) (get p :tag :absent)
             (get p "text") (get p 9223372036854775807)
             failed @events (get p 9)])"#,
                false
            ),
            "[true true 2 2 false nil 1.5 :wide :threw [:receiver :key :value] 9]"
        );
    }
}

#[test]
fn private_registration_is_atomic_and_ordinary_core_declines() {
    for mut session in sessions() {
        for source in [
            "(suss.bootstrap/transient-map-types! nil nil nil nil nil)",
            "(suss.bootstrap/transient-map-assoc nil nil nil)",
            "(suss.bootstrap/transient-map-conj nil nil)",
        ] {
            let Err(SessionError::Compile(error)) = session.eval(source) else {
                panic!("private adapter was not rejected: {source}");
            };
            assert_eq!(
                error.message,
                "Private transient map adapters require compiled core"
            );
        }
        session.eval("(ns suss.core)").unwrap();
        assert_eq!(
            printed(
                &mut session,
                r#"
          (let [events (atom []) t (transient {1 2})]
            [(suss.bootstrap/transient-map-types! nil nil nil nil (do (swap! events conj :last) nil))
             (suss.bootstrap/undefined? (suss.bootstrap/transient-map-assoc t 1 3))
             (suss.bootstrap/undefined? (suss.bootstrap/transient-map-conj t [1 4]))
             (persistent! t) @events])"#,
                false
            ),
            "[nil true true {1 2} [:last]]"
        );
        session.eval("(ns user)").unwrap();
        assert!(
            session
                .eval_privileged(
                    r#"
          (suss.bootstrap/transient-map-types! (transient []) (transient [])
             (transient []) (transient []) nil)"#
                )
                .is_err()
        );
        assert_eq!(
            printed(
                &mut session,
                r#"
          (let [e (Unique.) t (TransientHashMap. e (BitmapIndexedNode. e 0 (make-array 8)) 0 false nil)]
            [(suss.bootstrap/undefined? (suss.bootstrap/transient-map-conj t [1 :ok]))
             (persistent! t)])"#,
                true
            ),
            "[false {1 :ok}]"
        );
    }
}
