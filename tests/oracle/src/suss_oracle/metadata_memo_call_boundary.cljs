(ns suss-oracle.metadata-memo-call-boundary
  (:require [cljs.analyzer :as ana] [cljs.env :as env]
            [cljs.js :as cljs] [cljs.tools.reader :as reader]))

;; Development-only draft derived from af09b1b metadata_memo_boundary.cljs.
;; No Suss transport or acceptance. No recursion, cancellation, or concurrency.
(defn intended-cause [caught intended]
  ;; The analyzer wraps macro exceptions in ExceptionInfo. Accept only the
  ;; exact Error object we threw, through a bounded standard cause chain.
  (loop [e caught depth 0 chain []]
    (cond
      (identical? e intended) (conj chain (.-name e))
      (or (nil? e) (>= depth 16)) nil
      :else (recur (ex-cause e) (inc depth)
              (conj chain (if (instance? js/Error e) (.-name e) "non-Error"))))))

(defn source-control []
  (let [state (cljs/empty-state)
        target 'suss-oracle.memo-call-source-control]
    (swap! state assoc-in [::ana/namespaces target] {:name target :defs {}})
    (binding [env/*compiler* state ana/*cljs-ns* target
              ana/*analyze-deps* false ana/*load-macros* false]
      (let [ast (ana/analyze (assoc (ana/empty-env) :ns (ana/get-namespace target))
                  (reader/read-string "(def ^{:audit {:x 17}} watched 17)"))
            m (get-in @state [::ana/namespaces target :defs 'watched :meta])]
        (assert (= 17 (:val (:init ast))))
        (assert (= 17 (:x (:audit m))))
        #js {:declaration-value (:val (:init ast)) :audit-x (:x (:audit m))}))))

(defn run-case [mode]
  (when (nil? cljs.core/NS_CACHE) (set! cljs.core/NS_CACHE (atom {})))
  (let [state (cljs/empty-state) cache cljs.core/NS_CACHE old-cache @cache
        target 'suss-oracle.memo-call-target
        macro-lib 'suss-oracle.memo-call-macros
        macro-ns (symbol (str macro-lib "$macros"))
        proto (.-prototype cljs.core/PersistentArrayMap)
        method "cljs$core$IHash$_hash$arity$1" original (aget proto method)
        subject (array nil) saved-meta (array nil)
        action (array nil) stage (array "baseline")
        dispatches (array 0) callbacks (array 0) macro-entries (array 0)
        events (array) observations (array) event-labels (array)
        intended (doto (js/Error. "memo-call-boundary intended throw")
                   (aset "name" "MemoCallBoundaryError"))
        exception-record (array nil)
        emit (fn [label]
               (let [a (aget subject 0)]
                 (.push event-labels label)
                 (.push events #js {:event label :stage (aget stage 0)
                                    :x (:x a) :memo (.-__hash a)
                                    :subject-dispatches (aget dispatches 0)
                                    :nested-callbacks (aget callbacks 0)})))
        analyze (fn [form]
                  (binding [env/*compiler* state ana/*cljs-ns* target
                            ana/*analyze-deps* false ana/*load-macros* false]
                    (ana/analyze (assoc (ana/empty-env) :ns (ana/get-namespace target)) form)))
        observer (fn [_ macro-env]
                   (aset macro-entries 0 (inc (aget macro-entries 0)))
                   ;; Deliberately no catch here: the intended Error must leave
                   ;; this macro, pass through analyzer wrapping, and reach runner.
                   (when-let [f (aget action 0)] (f))
                   (let [stored (get-in macro-env [:ns :defs 'watched])
                         resolved (ana/resolve-var macro-env 'watched)
                         info (:info (ana/analyze macro-env 'watched))
                         m (aget saved-meta 0) a (aget subject 0)]
                     (.push observations
                       #js {:stage (aget stage 0)
                            :env-meta (identical? m (:meta stored))
                            :resolved-meta (identical? m (:meta resolved))
                            :ast-meta (identical? m (:meta info))
                            :env-audit (identical? a (:audit (:meta stored)))
                            :resolved-audit (identical? a (:audit (:meta resolved)))
                            :ast-audit (identical? a (:audit (:meta info)))
                            :old-x (:x a) :current-x (:x (:audit (:meta info)))
                            :old-memo (.-__hash a)
                            :current-memo (.-__hash (:audit (:meta info)))
                            :subject-dispatches (aget dispatches 0)
                            :nested-callbacks (aget callbacks 0)})
                     17))
        invoke (fn []
                 (let [ast (analyze (reader/read-string
                                    "(suss-oracle.memo-call-macros/observe)"))]
                   (assert (= :const (:op ast)))
                   (assert (= 17 (:val ast)))))]
    (assert (contains? #{:nested-write :write-poison-restore :write-throw} mode))
    (assert (fn? original))
    (try
      (swap! state assoc-in [::ana/namespaces target] {:name target :defs {}})
      (swap! state assoc-in [::ana/namespaces macro-ns]
        {:name macro-ns :defs {'observe {:name (symbol (str macro-ns) "observe")
                                       :macro true :method-params [['&form '&env]] :max-fixed-arity 2}}})
      (set! (.-cljs$lang$macro observer) true)
      (swap! cache assoc macro-ns (create-ns macro-ns #js {:observe observer}))
      (analyze (reader/read-string "(def ^{:audit {:x 17}} watched 17)"))
      (let [m (get-in @state [::ana/namespaces target :defs 'watched :meta])
            a (:audit m) arr (.-arr a) canonical-control (hash {:x 17})
            nested-write (fn [receiver]
                           (aset callbacks 0 (inc (aget callbacks 0)))
                           (emit "callback-enter")
                           (set! (.-__hash receiver) 91)
                           (emit "callback-write"))
            replacement (fn [coll]
                          (this-as receiver
                            (if (identical? receiver (aget subject 0))
                              (do
                                (aset dispatches 0 (inc (aget dispatches 0)))
                                (emit "hash-enter")
                                (if (= mode :nested-write)
                                  (nested-write receiver)
                                  (do (set! (.-__hash receiver) 91) (emit "method-write")))
                                (when (= mode :write-poison-restore)
                                  (aset arr 1 99) (emit "method-poison")
                                  (aset arr 1 17) (emit "method-restore"))
                                (when (= mode :write-throw)
                                  (emit "method-throw")
                                  (throw intended))
                                (emit "hash-return")
                                91)
                              (.call original receiver coll))))]
        (assert (instance? cljs.core/PersistentArrayMap a))
        (assert (= 2 (.-length arr))) (assert (= :x (aget arr 0)))
        (assert (= 17 (:x a))) (assert (nil? (.-__hash a)))
        (aset subject 0 a) (aset saved-meta 0 m)
        (invoke)
        (aset proto method replacement)
        (aset stage 0 "hash-window")
        (aset action 0 (fn []
                        (emit "macro-before-hash")
                        (let [result (hash a)]
                          (assert (= 91 result))
                          (emit "macro-after-hash"))))
        (if (= mode :write-throw)
          (do
            (try
              (invoke)
              (catch :default e
                (if-let [chain (intended-cause e intended)]
                  (do
                    (assert (= "MemoCallBoundaryError" (.-name intended)))
                    (assert (= "memo-call-boundary intended throw" (.-message intended)))
                    (aset exception-record 0
                      #js {:escaped-macro true :exact-intended-cause true
                           :name (.-name intended) :message (.-message intended)
                           :cause-chain (into-array chain)})
                    (emit "runner-caught-intended"))
                  (throw e))))
            (assert (some? (aget exception-record 0)) "intended error did not escape macro"))
          (invoke))
        ;; No repair or hash call between the exception and next macro entry.
        (aset action 0 nil) (aset stage 0 "next-invocation") (invoke)
        (assert (= 3 (aget macro-entries 0)))
        (assert (= 1 (aget dispatches 0)))
        (assert (= (if (= mode :nested-write) 1 0) (aget callbacks 0)))
        (assert (= 91 (.-__hash a))) (assert (= 17 (:x a)))
        (assert (= (if (= mode :write-throw) 2 3) (.-length observations)))
        (assert (= (vec (array-seq event-labels))
                   (case mode
                     :nested-write ["macro-before-hash" "hash-enter" "callback-enter"
                                    "callback-write" "hash-return" "macro-after-hash"]
                     :write-poison-restore ["macro-before-hash" "hash-enter" "method-write"
                                            "method-poison" "method-restore" "hash-return" "macro-after-hash"]
                     :write-throw ["macro-before-hash" "hash-enter" "method-write"
                                   "method-throw" "runner-caught-intended"])))
        #js {:mode (name mode) :canonical-control canonical-control
             :boundary "Pinned CLJS analyzer and registered compiled macro; no Suss transport"
             :subject-dispatches (aget dispatches 0) :nested-callbacks (aget callbacks 0)
             :macro-entries (aget macro-entries 0)
             :method-installed (identical? replacement (aget proto method))
             :exception (aget exception-record 0) :events events :observations observations})
      (finally (aset proto method original) (reset! cache old-cache)))))

(defn -main []
  (let [before (source-control)
        cases (into-array (map run-case [:nested-write :write-poison-restore :write-throw]))
        after (source-control)]
    (println (.stringify js/JSON
      #js {:schema 1 :source-controls #js [before after] :cases cases
           :limits #js ["Development-only CLJS reference draft; no Suss acceptance."
                         "Nested callback is an ordinary function call inside IHash, not recursive hash dispatch."
                         "Identity and event observations concern these three synchronous cases only."
                         "No cancellation, concurrency, or recursive hash calls exercised."]}))))
(set! *main-cli-fn* -main)
