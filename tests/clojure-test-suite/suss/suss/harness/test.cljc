(ns suss.harness.test
  "Runtime half of the conformance harness's clojure.test replacement.

  Assertions record raw values for host-side decoding; nothing here decides
  whether Suss passes. The host runs one registered test at a time and drains
  `observations` after each run.")

;; Harness functions use (def name (fn ...)) rather than defn: compiled defn
;; expansion currently grows quadratically with preceding definitions (#14).

;; Registered tests in definition order: [name-symbol test-fn].
(def tests (atom []))
;; Each observation is [kind verdict payload]; see suss.harness.test-macros/is.
(def observations (atom []))
;; Skipped symbols in record order.
(def skips (atom []))
;; [thrown? thrown-value] of the most recent run-test!.
(def outcome (atom [false nil]))

(def register!
  (fn register! [name f]
    (swap! tests conj [name f])
    nil))

(def record!
  (fn record! [kind verdict payload]
    (swap! observations conj [kind verdict payload])
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

(def run-test!
  (fn run-test! [index]
    (reset! observations [])
    (reset! skips [])
    (let [f (nth (nth @tests index) 1)]
      (reset! outcome (try
                        (f)
                        [false nil]
                        (catch :default error
                          [true error])))
      nil)))

(def run-outcome
  (fn run-outcome []
    @outcome))

;; Skips recorded outside a test run (namespace load), cleared on read.
(def drain-skips!
  (fn drain-skips! []
    (let [recorded @skips]
      (reset! skips [])
      recorded)))

(def observation
  (fn observation [index]
    (nth @observations index)))

(def operand
  (fn operand [index position]
    (nth (nth (nth @observations index) 2) position)))
