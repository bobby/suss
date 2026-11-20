//! Expected effects were observed on JVM Clojure 1.12.6 (#482).
use suss_cli::portable_session::Session;

fn printed(session: &mut Session, source: &str) -> String {
    printed_in(session, source, false)
}

fn printed_in(session: &mut Session, source: &str, privileged: bool) -> String {
    let form = format!("(pr-str {source})");
    let result = if privileged { session.eval_privileged(&form) } else { session.eval(&form) };
    let value = result.unwrap_or_else(|error| panic!("{source}: {error}"));
    session.collect().unwrap();
    let units = session.inspect(&value, |mut store, value| {
        let array = value.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
        Ok(array.elems(&mut store)?.map(|unit| unit.unwrap_i32() as u16).collect::<Vec<_>>())
    }).unwrap();
    String::from_utf16(&units).unwrap()
}

#[test]
fn chunk_buffer_matches_jvm_returns_and_failed_store_order() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        assert_eq!(printed(&mut session, r#"
          [(let [b (chunk-buffer 2) a (chunk-append b :a) z (chunk-append b :b) c (chunk b)]
             [a z (count b) (count c) (nth c 0) (nth c 1)])
           (let [b (chunk-buffer 0)
                 threw (try (chunk-append b :x) false (catch :default e true))]
             [threw (count b)])
           (let [b (chunk-buffer 1) _ (chunk-append b :first)
                 threw (try (chunk-append b :second) false (catch :default e true))]
             [threw (count b)])
           (let [b (chunk-buffer 2) _ (chunk-append b :first) c (chunk b)
                 threw (try (chunk-append b :second) false (catch :default e true))]
             [threw (count b) (nth c 0)])]"#),
          "[[nil nil 2 2 :a :b] [true 1] [true 2] [true 2 :first]]");
    }
}

#[test]
fn chunked_filter_keeps_the_value_read_before_its_predicate() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        // The JVM adapter constructs clojure.lang.ArrayChunk from the same
        // Object[]; array-chunk is Suss's implementation-side factory.
        assert_eq!(printed(&mut session, r#"
          (let [a (object-array [1 2]) events (atom [])
                s (chunk-cons (array-chunk a) nil)
                out (vec (filter (fn [x]
                                  (swap! events conj x)
                                  (when (= x 1) (aset a 0 99))
                                  true) s))]
            [out @events (vec a)])"#), "[[1 2] [1 2] [99 2]]");
    }
}

#[test]
fn custom_chunks_keep_their_indexed_protocol_effects() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.eval(r#"
          (deftype ReadChunk [xs events]
            ICounted (-count [_] (count xs))
            IIndexed
            (-nth [_ i] (swap! events conj i) (nth xs i))
            (-nth [_ i missing] (swap! events conj i) (nth xs i missing))
            IChunk (-drop-first [_] (ReadChunk. (subvec xs 1) events))
            IReduce
            (-reduce [_ f] (reduce f xs))
            (-reduce [_ f start] (reduce f start xs)))"#).unwrap();
        session.collect().unwrap();
        // JVM IChunk/Indexed adapter, using a pure predicate, produces this
        // exact trace: one custom indexed read for every input item.
        assert_eq!(printed(&mut session, r#"
          (let [events (atom []) c (ReadChunk. [1 2 3] events)]
            [(vec (filter odd? (chunk-cons c nil))) @events])"#),
          "[[1 3] [0 1 2]]");
    }
}

#[test]
fn native_chunk_admission_keeps_nil_end_bounds_and_decline_effects() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        // Privileges expose private adapters; observations are values/effects.
        assert_eq!(printed_in(&mut session, r#"
          (let [c (array-chunk (object-array [0 nil 20]) 1 2)]
            [(suss.bootstrap/undefined? (suss.bootstrap/chunk-nth c 0))
             (nil? (suss.bootstrap/chunk-nth c 0))
             (suss.bootstrap/chunk-nth c 1)
             (suss.bootstrap/undefined? (suss.bootstrap/chunk-nth c -1))
             (suss.bootstrap/undefined? (suss.bootstrap/chunk-nth c 0.5))
             (suss.bootstrap/undefined? (suss.bootstrap/chunk-nth c 99))
             (suss.bootstrap/undefined? (suss.bootstrap/chunk-nth nil 0))])"#, true),
          "[false true 20 true true true true]");
        assert_eq!(printed_in(&mut session, r#"
          (let [b (chunk-buffer 0)
                declined (suss.bootstrap/undefined? (suss.bootstrap/chunk-append b :x))
                before (count b)
                threw (try (chunk-append b :x) false (catch :default e true))]
            [declined before threw (count b)])"#, true), "[true 0 true 1]");
        // Suss's primitive-backed implementation domain keeps its original
        // generic path; the native Object[] adapter must decline it.
        assert_eq!(printed_in(&mut session, r#"
          (let [c (array-chunk (long-array [5 6]) 0 2)]
            [(suss.bootstrap/undefined? (suss.bootstrap/chunk-nth c 0))
             (vec (map identity (chunk-cons c nil)))])"#, true), "[true [5 6]]");
    }
}

#[test]
fn native_chunk_roots_and_shallow_elements_survive_gc() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.eval("(def kept-element (object-array [7])) (def kept-buffer (chunk-buffer 1))
                      (def kept-chunk (array-chunk (object-array [nil kept-element]) 1 2))").unwrap();
        session.collect().unwrap();
        assert_eq!(printed_in(&mut session, r#"
          [(identical? (suss.bootstrap/chunk-nth kept-chunk 0) kept-element)
           (nil? (suss.bootstrap/chunk-append kept-buffer kept-element))
           (count kept-buffer)]"#, true), "[true true 1]");
        session.collect().unwrap();
        assert_eq!(printed(&mut session,
          "(let [c (chunk kept-buffer)] [(identical? (nth c 0) kept-element) (count c)])"),
          "[true 1]");
    }
}

#[test]
fn chunk_views_read_future_elements_after_callbacks_and_keep_reduced_demand() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        assert_eq!(printed(&mut session, r#"
          [(let [a (object-array [99 nil 2 3]) events (atom [])]
             [(vec (map (fn [x] (swap! events conj x)
                                   (when (nil? x) (aset a 2 20)) x)
                        (chunk-cons (array-chunk a 1 4) nil))) @events])
           (let [a (object-array [99 nil 2 3]) events (atom [])]
             [(vec (filter (fn [x] (swap! events conj x)
                                      (when (nil? x) (aset a 2 20)) true)
                           (chunk-cons (array-chunk a 1 4) nil))) @events])
           (let [a (object-array [99 nil 2 3]) events (atom [])
                 more (LazySeq. nil (fn [] (swap! events conj :tail) (list 999)) nil nil)]
             [(reduce (fn [acc x] (swap! events conj x)
                                  (when (nil? x) (aset a 2 20))
                                  (if (= x 20) (reduced (conj acc x)) (conj acc x)))
                      [] (chunk-cons (array-chunk a 1 4) more)) @events])]"#),
          "[[[nil 20 3] [nil 20 3]] [[nil 20 3] [nil 20 3]] [[nil 20] [nil 20]]]");
    }
}

#[test]
fn chunk_storage_views_decline_unsupported_windows_and_keep_aliases_after_gc() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.eval("(def view-array (object-array [0 nil 2]))
                      (def view-chunk (array-chunk view-array 1 2))").unwrap();
        session.collect().unwrap();
        assert_eq!(printed_in(&mut session, r#"
          [(identical? (suss.bootstrap/chunk-array view-chunk) view-array)
           (suss.bootstrap/chunk-offset view-chunk)
           (aget (suss.bootstrap/chunk-array view-chunk) 1)
           (suss.bootstrap/undefined? (suss.bootstrap/chunk-array nil))
           (suss.bootstrap/undefined? (suss.bootstrap/chunk-array
                                       (array-chunk (long-array [1 2]))))
           (suss.bootstrap/undefined? (suss.bootstrap/chunk-array
                                       (array-chunk view-array -1 2)))
           (suss.bootstrap/undefined? (suss.bootstrap/chunk-array
                                       (array-chunk view-array 1 4)))]"#, true),
          "[true 1 nil true true true true]");
    }
}

#[test]
fn invalid_chunk_windows_keep_callbacks_before_the_first_bad_read() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        assert_eq!(printed(&mut session, r#"
          [(let [events (atom []) c (array-chunk (object-array [1 2]) 0 3)
                 threw (try (vec (map (fn [x] (swap! events conj x) x)
                                      (chunk-cons c nil)))
                            false (catch :default e true))]
             [threw @events])
           (let [events (atom []) c (array-chunk (object-array [1 2]) 0 3)
                 threw (try (vec (filter (fn [x] (swap! events conj x) true)
                                         (chunk-cons c nil)))
                            false (catch :default e true))]
             [threw @events])
           (let [events (atom []) c (array-chunk (object-array [1 2]) 0 3)
                 threw (try (reduce (fn [acc x] (swap! events conj x) (conj acc x))
                                    [] (chunk-cons c nil))
                            false (catch :default e true))]
             [threw @events])]"#), "[[true [1 2]] [true [1 2]] [true [1 2]]]");
    }
}
