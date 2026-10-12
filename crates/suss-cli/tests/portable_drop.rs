//! Pinned ClojureScript `drop` sequence behavior in the portable runtime.
use suss_cli::portable_session::Session;
use wasmtime::Val;

fn number(session: &mut Session, source: &str) -> f64 {
    let value = session.eval(source).unwrap_or_else(|error| panic!("{source}: {error:?}"));
    session.inspect(&value, |mut store, value| {
        let fields = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap().fields(&mut store)?.collect::<Vec<_>>();
        let [Val::F64(bits)] = fields.as_slice() else { panic!("numeric result: {fields:?}") };
        Ok(f64::from_bits(*bits))
    }).unwrap()
}

#[test]
fn drop_preserves_idrop_lazy_sequence_and_transducer_contracts() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.set_operation_fuel(100_000_000);
        assert_eq!(number(&mut session, "(count (drop 2 [0 1 2 3]))"), 2.0);
        assert_eq!(number(&mut session, "(first (drop 2 [0 1 2 3]))"), 2.0);
        assert_eq!(number(&mut session, "(first (drop -2 [7 8]))"), 7.0);
        assert_eq!(number(&mut session, "(count (drop 9 [0 1]))"), 0.0);
        assert_eq!(number(&mut session, "(count (drop 1.2 [0 1 2 3]))"), 2.0);
        assert_eq!(number(&mut session, "(reduce + 0 (drop 2 (range 6)))"), 14.0);
        assert_eq!(number(&mut session, "(transduce (drop 2) + 0 [0 1 2 3])"), 5.0);
        assert_eq!(number(&mut session, "(if (try (drop nil [1]) false (catch :default e true)) 1 0)"), 1.0);
        assert_eq!(number(&mut session, "(if (try (drop nil) false (catch :default e true)) 1 0)"), 1.0);
        session.eval("(def doall-effects 0) (def doall-seq (map (fn [x] (set! doall-effects (+ doall-effects 1)) x) [1 2 3]))").unwrap();
        assert_eq!(number(&mut session, "(if (identical? doall-seq (doall doall-seq)) 1 0)"), 1.0);
        assert_eq!(number(&mut session, "doall-effects"), 3.0);
        session.eval("(def dorun-effects 0) (def dorun-seq (map (fn [x] (set! dorun-effects (+ dorun-effects 1)) x) (list 1 2 3 4)))").unwrap();
        assert_eq!(number(&mut session, "(if (identical? dorun-seq (doall 2 dorun-seq)) 1 0)"), 1.0);
        assert_eq!(number(&mut session, "dorun-effects"), 3.0);
        assert_eq!(number(&mut session, "(if (nil? (dorun [1 2])) 1 0)"), 1.0);
        session.eval("(def drop-map-seq (doall (drop 1 {:a 1 :b 2 :c 3})))").unwrap();
        assert_eq!(number(&mut session, "(if (map-entry? (first drop-map-seq)) 1 0)"), 1.0);
        assert_eq!(number(&mut session, "(if (= (get {:a 1 :b 2 :c 3} (key (first drop-map-seq))) (val (first drop-map-seq))) 1 0)"), 1.0);
        assert_eq!(number(&mut session, "(if (= 2 (count drop-map-seq)) 1 0)"), 1.0);
        session.eval("(def drop-set #{:c :b :a}) (def drop-set-seq (seq drop-set))").unwrap();
        assert_eq!(number(&mut session, "(if (= (disj drop-set (first drop-set-seq)) (set (drop 1 drop-set))) 1 0)"), 1.0);
    }
}
