(ns suss.harness.test
  "Runtime half of the conformance harness's clojure.test replacement.

  Assertions record raw values for host-side decoding; nothing here decides
  whether Suss passes. The host loads every namespace, then runs each
  namespace's tests as one group under its fixtures, as cljs.test does, and
  decodes each test's recorded observations.")

;; Harness functions use (def name (fn ...)) rather than defn: compiled defn
;; expansion currently grows quadratically with preceding definitions (#14).

;; Registered tests in definition order: [name-symbol test-fn].
(def tests (atom []))
;; Fixtures registered since the host last attached them: [kind fixture-fn].
(def pending-fixtures (atom []))
;; Attached fixtures by host namespace index.
(def namespace-fixtures (atom {}))
;; Skips recorded since the host last drained them.
(def skips (atom []))
;; Observations of the running test: [kind verdict payload head].
(def observations (atom []))
;; Per test of the last group: [threw? thrown-value observations skips].
(def results (atom []))
;; [threw? thrown-value] of the last group's once-fixtures.
(def group-outcome (atom [false nil]))

(def register!
  (fn register! [name f]
    (swap! tests conj [name f])
    nil))

(def use-fixtures!
  (fn use-fixtures! [kind fixtures]
    (loop [remaining (seq fixtures)]
      (when remaining
        (swap! pending-fixtures conj [kind (first remaining)])
        (recur (next remaining))))
    nil))

(def attach-fixtures!
  (fn attach-fixtures! [namespace-index]
    (swap! namespace-fixtures assoc namespace-index @pending-fixtures)
    (reset! pending-fixtures [])
    nil))

(def record!
  (fn record! [kind verdict payload head]
    (swap! observations conj [kind verdict payload head])
    nil))

(def skip!
  (fn skip! [symbol]
    (swap! skips conj symbol)
    nil))

(def test-count
  (fn test-count []
    (count @tests)))

(def test-name
  (fn test-name [index]
    (nth (nth @tests index) 0)))

;; cljs.test composes fixtures so the first registered is outermost.
(def compose
  (fn compose [fixtures body]
    (let [fixtures (vec fixtures)]
      (loop [index (dec (count fixtures)) f body]
        (if (neg? index)
          f
          (recur (dec index)
                 (let [fixture (nth fixtures index) inner f]
                   (fn [] (fixture inner)))))))))

(def fixtures-of
  (fn fixtures-of [fixtures kind]
    (loop [remaining (seq fixtures) result []]
      (if remaining
        (recur (next remaining)
               (if (= kind (nth (first remaining) 0))
                 (conj result (nth (first remaining) 1))
                 result))
        result))))

(def run-one!
  (fn run-one! [index each]
    (reset! observations [])
    (reset! skips [])
    (let [f (compose each (nth (nth @tests index) 1))
          outcome (try
                    (f)
                    [false nil]
                    (catch :default error
                      [true error]))]
      (swap! results conj [(nth outcome 0) (nth outcome 1) @observations @skips]))))

(def run-group!
  "Run tests `indices` of one namespace under its attached fixtures."
  (fn run-group! [indices namespace-index]
    (reset! results [])
    (let [fixtures (get @namespace-fixtures namespace-index [])
          each (fixtures-of fixtures :each)
          body (fn []
                 (loop [remaining (seq indices)]
                   (when remaining
                     (run-one! (first remaining) each)
                     (recur (next remaining)))))]
      (reset! group-outcome (try
                              ((compose (fixtures-of fixtures :once) body))
                              [false nil]
                              (catch :default error
                                [true error]))))
    nil))

(def drain-skips!
  (fn drain-skips! []
    (let [recorded @skips]
      (reset! skips [])
      recorded)))

(def result-count
  (fn result-count []
    (count @results)))

(def group-field
  (fn group-field [field]
    (nth @group-outcome field)))

(def result-field
  (fn result-field [test field]
    (nth (nth @results test) field)))

(def observation
  (fn observation [test index]
    (nth (nth (nth @results test) 2) index)))

(def operand
  (fn operand [test index position]
    (nth (nth (observation test index) 2) position)))
