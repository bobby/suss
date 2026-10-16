(ns suss-oracle.metadata-memo-boundary
  (:require [cljs.analyzer :as ana] [cljs.env :as env]
            [cljs.js :as cljs] [cljs.tools.reader :as reader]))

;; Development-only pinned observation. No Suss transport. Each case owns a fresh
;; compiler state; restoration in finally also covers failed assertions.
(defn run-case [mode]
  (when (nil? cljs.core/NS_CACHE) (set! cljs.core/NS_CACHE (atom {})))
  (let [state (cljs/empty-state) cache cljs.core/NS_CACHE old-cache @cache
        target-ns 'suss-oracle.memo-boundary-target
        macro-lib 'suss-oracle.memo-boundary-macros
        macro-ns (symbol (str macro-lib "$macros"))
        proto (.-prototype cljs.core/PersistentArrayMap)
        method "cljs$core$IHash$_hash$arity$1" original (aget proto method)
        subject (array nil) saved-meta (array nil)
        dispatches (array 0) action (array nil) label (array "baseline")
        events (array) observations (array)
        analyze (fn [form]
                  (binding [env/*compiler* state ana/*cljs-ns* target-ns
                            ana/*analyze-deps* false ana/*load-macros* false]
                    (ana/analyze (assoc (ana/empty-env) :ns (ana/get-namespace target-ns)) form)))
        observer (fn [_ macro-env]
                   ;; Mutations/hash run INSIDE an actual macro invocation.
                   (when-let [f (aget action 0)] (f))
                   (let [stored (get-in macro-env [:ns :defs 'watched])
                         resolved (ana/resolve-var macro-env 'watched)
                         info (:info (ana/analyze macro-env 'watched))
                         m (aget saved-meta 0) a (aget subject 0)]
                     (.push observations
                       #js {:stage (aget label 0)
                            :env-meta (identical? m (:meta stored))
                            :resolved-meta (identical? m (:meta resolved))
                            :ast-meta (identical? m (:meta info))
                            :env-audit (identical? a (:audit (:meta stored)))
                            :resolved-audit (identical? a (:audit (:meta resolved)))
                            :ast-audit (identical? a (:audit (:meta info)))
                            :old-x (:x a) :current-x (:x (:audit (:meta info)))
                            :old-memo (.-__hash a)
                            :current-memo (.-__hash (:audit (:meta info)))
                            :subject-dispatches (aget dispatches 0)})
                     17))
        invoke (fn []
                 (let [ast (analyze (reader/read-string
                                    "(suss-oracle.memo-boundary-macros/observe)"))]
                   (assert (= :const (:op ast))) (assert (= 17 (:val ast)))))]
    (assert (fn? original))
    (swap! state assoc-in [::ana/namespaces target-ns] {:name target-ns :defs {}})
    (swap! state assoc-in [::ana/namespaces macro-ns]
      {:name macro-ns :defs {'observe {:name (symbol (str macro-ns) "observe")
                                     :macro true :method-params [['&form '&env]] :max-fixed-arity 2}}})
    (set! (.-cljs$lang$macro observer) true)
    (swap! cache assoc macro-ns (create-ns macro-ns #js {:observe observer}))
    (try
      (analyze (reader/read-string "(def ^{:audit {:x 17}} watched 17)"))
      (let [m (get-in @state [::ana/namespaces target-ns :defs 'watched :meta])
            a (:audit m) arr (.-arr a)
            initial-x (:x a) initial-memo (.-__hash a)
            canonical-control (hash {:x 17})
            custom? (contains? #{:custom-write :poison-custom-restore} mode)
            poisoned? (contains? #{:poison-custom-restore :poison-direct-restore :poison-unrestored} mode)
            replacement (fn [coll]
                          (this-as receiver
                            (if (identical? receiver (aget subject 0))
                              (do
                                (aset dispatches 0 (inc (aget dispatches 0)))
                                (let [memo (.-__hash receiver)]
                                  (if (nil? memo)
                                    (do (set! (.-__hash receiver) 91) 91)
                                    memo)))
                              (.call original receiver coll))))]
        (assert (instance? cljs.core/PersistentArrayMap a))
        (assert (= 2 (.-length arr))) (assert (= :x (aget arr 0)))
        (assert (= 17 initial-x)) (assert (nil? initial-memo))
        (aset saved-meta 0 m) (aset subject 0 a)
        (invoke)
        (when custom? (aset proto method replacement))
        (aset label 0 "mutation-window")
        (aset action 0
          (fn []
            (when poisoned? (aset arr 1 99))
            (let [x-at-write (:x a) memo-before (.-__hash a)
                  result (if custom? (hash a)
                           (do (set! (.-__hash a) 991) 991))
                  memo-after (.-__hash a)]
              (.push events #js {:x-at-write x-at-write :memo-before memo-before
                                 :result result :memo-after memo-after
                                 :subject-dispatches (aget dispatches 0)}))
            (when (contains? #{:poison-custom-restore :poison-direct-restore} mode)
              (aset arr 1 17))))
        (invoke)
        (aset action 0 nil) (aset label 0 "next-invocation") (invoke)
        #js {:mode (name mode) :canonical-control canonical-control
             :boundary "Pinned CLJS analyzer and bootstrap-registered compiled macro; no Suss transport"
             :initial-x initial-x :initial-memo initial-memo
             :method-installed (when custom? (identical? replacement (aget proto method)))
             :events events :observations observations})
      (finally (aset proto method original) (reset! cache old-cache)))))

(defn source-control []
  (let [state (cljs/empty-state) target-ns 'suss-oracle.memo-boundary-source-control]
    (swap! state assoc-in [::ana/namespaces target-ns] {:name target-ns :defs {}})
    (binding [env/*compiler* state ana/*cljs-ns* target-ns
              ana/*analyze-deps* false ana/*load-macros* false]
      (let [ast (ana/analyze (assoc (ana/empty-env) :ns (ana/get-namespace target-ns))
                  (reader/read-string "(def ^{:audit {:x 17}} watched 17)"))
            m (get-in @state [::ana/namespaces target-ns :defs 'watched :meta])]
        (assert (= 17 (:val (:init ast))))
        (assert (= 17 (:x (:audit m))))
        #js {:declaration-value (:val (:init ast)) :audit-x (:x (:audit m))}))))

(defn -main []
  (let [before (source-control)
        cases (into-array (map run-case [:custom-write :direct-write
                                        :poison-custom-restore :poison-direct-restore
                                        :poison-unrestored]))
        after (source-control)]
    (println (.stringify js/JSON
      #js {:schema 1 :source-controls #js [before after] :cases cases}))))
(set! *main-cli-fn* -main)
