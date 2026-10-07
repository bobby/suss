(ns suss.harness.test-macros
  "Macro half of the conformance harness's clojure.test replacement.

  Expansions preserve cljs.test evaluation: `is` evaluates predicate operands
  left to right exactly once, then applies the predicate; other forms evaluate
  as written; any throw becomes an :error observation. Payloads are vectors:
  predicate [result operand...], value [result], thrown [threw?], error
  [thrown-value]. Whether a predicate is reported by operands or by result is
  decided host-side from the oracle's own assertion kind.")

;; Heads cljs.test never treats as function predicates: analyzer special
;; forms, cljs.core macros without a same-named runtime var, and instance?,
;; which has its own assert-expr. scripts/clojure_test_suite.py verifies this
;; set against docs/compatibility/cljs-core.edn.
(def non-functions
  '#{& -> ->> . .. amap and areduce as-> assert binding caching-hash case case*
    coercive-= coercive-boolean coercive-not coercive-not= comment cond cond->
    cond->> condp copy-arguments declare def defcurried defmethod defmulti defn-
    defonce defprotocol defrecord defrecord* deftype deftype* delay divide do
    doseq dotimes doto es6-iterable exists? extend-protocol extend-type fn fn*
    for gen-apply-to gen-apply-to-simple goog-define if if-let if-not if-some
    implements? import import-macros instance? js* js-arguments js-comment
    js-debugger js-fn? js-in js-inline-comment js-str js-this lazy-cat lazy-seq
    let let* letfn letfn* load-file* locking loop loop* macroexpand
    macroexpand-1 memfn new ns ns* ns-imports ns-interns ns-publics
    ns-special-form ns-unmap or quote recur refer-clojure refer-global reify
    require require-global require-macros resolve return-first rfn satisfies?
    set! simple-benchmark some-> some->> specify specify! this-as throw time try
    unchecked-get unchecked-max unchecked-min unchecked-set unsafe-bit-and
    unsafe-cast use use-macros var vswap! when when-assert when-first when-let
    when-not when-some while with-out-str with-redefs})

;; Helpers use (def name (fn ...)) rather than defn: compiled defn expansion
;; currently grows quadratically with preceding definitions (#14).
(def thrown-assertion?
  (fn thrown-assertion? [form]
    (and (seq? form) (symbol? (first form)) (= "thrown?" (name (first form))))))

(def predicate?
  (fn predicate? [form]
    (and (seq? form) (symbol? (first form))
         (not (and (nil? (namespace (first form)))
                   (contains? non-functions (first form)))))))

(defmacro testing [_label & body]
  `(do ~@body))

(defmacro use-fixtures [kind & fixtures]
  `(suss.harness.test/use-fixtures! ~kind [~@fixtures]))

(defmacro deftest [name & body]
  `(do
     (def ~name (fn ~name [] ~@body))
     (suss.harness.test/register! '~name ~name)))

(def mapv*
  (fn mapv* [f coll]
    (loop [items (seq coll) result []]
      (if items
        (recur (next items) (conj result (f (first items))))
        result))))

(def replace-template
  (fn replace-template [bindings form]
    (cond
      (and (symbol? form) (contains? bindings form)) (get bindings form)
      (seq? form) (apply list (mapv* (fn [x] (replace-template bindings x)) form))
      (vector? form) (mapv* (fn [x] (replace-template bindings x)) form)
      (map? form) (into {} (mapv* (fn [entry] [(replace-template bindings (key entry))
                                               (replace-template bindings (val entry))])
                                  form))
      (set? form) (into #{} (mapv* (fn [x] (replace-template bindings x)) form))
      :else form)))

(defmacro are
  "clojure.template/do-template over `is`, replacing argv symbols per group."
  [argv expr & args]
  (let [width (count argv)
        values (vec args)]
    (when-not (or (and (zero? width) (zero? (count values)))
                  (and (pos? width) (pos? (count values)) (zero? (mod (count values) width))))
      (throw (ex-info "The number of args doesn't match are's argv." {:argv argv})))
    (loop [start 0 forms []]
      (if (< start (count values))
        (let [group (subvec values start (+ start width))
              bindings (loop [i 0 m {}]
                         (if (< i width)
                           (recur (inc i) (assoc m (nth argv i) (nth group i)))
                           m))]
          (recur (+ start width) (conj forms (list 'suss.harness.test-macros/is
                                                   (replace-template bindings expr)))))
        (apply list 'do forms)))))

;; Syntax quote stays inside defmacro bodies: Suss does not yet expand
;; unquote-splicing in ordinary functions of a macro namespace. `is` is last
;; because later definitions' analysis environments would otherwise include its
;; large expansion and exceed the bounded macro analysis graph (issue #14).
;; Each observation also records the assertion's head symbol so the host can
;; detect assertions that diverge from the oracle's at the same ordinal. Like
;; cljs.test, `is` evaluates its message after the assertion and returns the
;; predicate result, the value, or (for thrown?) the thrown value.
(defmacro is
  ([form] `(is ~form nil))
  ([form message]
   (let [head (when (and (seq? form) (symbol? (first form))) (first form))]
     (cond
       (thrown-assertion? form)
       `(try
          (let [outcome# (try [false (do ~@(rest form))] (catch :default e# [true e#]))
                threw# (nth outcome# 0)]
            ~message
            (suss.harness.test/record! :thrown (if threw# :pass :fail) [threw#] '~head)
            (nth outcome# 1))
          (catch :default error#
            (suss.harness.test/record! :error :error [error#] '~head)
            error#))

       (predicate? form)
       `(try
          (let [values# (list ~@(rest form))
                result# (apply ~(first form) values#)]
            ~message
            (suss.harness.test/record! :predicate (if result# :pass :fail)
                                       (into [result#] values#) '~head)
            result#)
          (catch :default error#
            (suss.harness.test/record! :error :error [error#] '~head)
            error#))

       :else
       `(try
          (let [result# ~form]
            ~message
            (suss.harness.test/record! :value (if result# :pass :fail) [result#] '~head)
            result#)
          (catch :default error#
            (suss.harness.test/record! :error :error [error#] '~head)
            error#))))))
