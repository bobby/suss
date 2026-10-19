(ns suss-oracle.declaration-metadata-retention
  (:require [cljs.analyzer :as ana]
            [cljs.env :as env]
            [cljs.js :as cljs]
            [cljs.tools.reader :as reader]))

;; Original development-only observation of the pinned analyzer running as CLJS.
;; This does not simulate Suss's additional native-to-guest transport boundary.
(def probe-ns 'suss-oracle.retained-declaration)

(defn -main []
  (let [state (cljs/empty-state)
        original-hash cljs.core/hash
        trace (array)
        total (array 0)
        rows (array)
        run-window (fn [label f]
                     (set! (.-length trace) 0)
                     (aset total 0 0)
                     (let [value (f)
                           calls (.slice trace)]
                       (.push rows #js {:stage label :selected-numeric-hash-calls calls :total-hash-calls (aget total 0)})
                       value))
        analyze (fn [form]
                  (binding [env/*compiler* state
                            ana/*cljs-ns* probe-ns
                            ana/*analyze-deps* false
                            ana/*load-macros* false]
                    (ana/analyze (assoc (ana/empty-env)
                                  :ns (ana/get-namespace probe-ns)) form)))
        resolve-copy (fn [form]
                       (:init (first (:bindings (analyze form)))))
        checked-info (fn [init]
                       (let [info (:info init)]
                         (assert (= :var (:op init)) "Expected a resolved var initializer")
                         (assert (= (symbol (str probe-ns) "watched") (:name info))
                           "Expected the watched declaration")
                         (assert (and (map? info) (map? (:meta info)))
                           "Expected declaration info and metadata maps")
                         info))
        first-form (reader/read-string "(let* [copy watched] copy)")
        second-form (reader/read-string "(let* [copy watched] copy)")
        changed-copy-form (reader/read-string "(let* [copy watched] copy)")
        declaration-meta (fn []
                           (get-in @state [::ana/namespaces probe-ns :defs 'watched :meta]))]
    (swap! state assoc-in [::ana/namespaces probe-ns] {:name probe-ns :defs {}})
    (try
      (set! cljs.core/hash
        (fn [value]
          (aset total 0 (inc (aget total 0)))
          (when (and (number? value) (<= 101 value 109)) (.push trace value))
          (original-hash value)))
      (let [form (run-window "read-declaration"
                   #(reader/read-string
                      "(def ^{:audit #{101 102 103 104 105 106 107 108 109}} watched 17)"))
            _ (run-window "analyze-pre-read-declaration" #(analyze form))
            saved (declaration-meta)
            first-info (checked-info (run-window "resolve-first-pre-read" #(resolve-copy first-form)))
            second-info (checked-info (run-window "resolve-second-pre-read" #(resolve-copy second-form)))
            audit (:audit saved)]
        ;; Check data outside the measured windows: equality/lookup may hash.
        (assert (and (set? audit) (= [101 102 103 104 105 106 107 108 109]
                                    (vec (sort audit)))))
        (run-window "explicit-set-control" #(set [101 102 103 104 105 106 107 108 109]))
        (run-window "explicit-map-control" #(hash-map 101 :a 102 :b 103 :c 104 :d 105 :e 106 :f 107 :g 108 :h 109 :i))
        (let [occurrence (run-window "read-fresh-set-occurrence"
                           #(reader/read-string "#{101 102 103 104 105 106 107 108 109}"))]
          (run-window "analyze-pre-read-set-occurrence" #(analyze occurrence)))
        (let [changed-form (run-window "read-new-declaration"
                             #(reader/read-string "(def ^{:audit #{101 109}} watched 18)"))
              _ (run-window "analyze-new-declaration" #(analyze changed-form))
              changed-info (checked-info (run-window "resolve-new-revision-pre-read" #(resolve-copy changed-copy-form)))]
          (assert (and (set? (:audit (:meta changed-info)))
                       (= [101 109] (vec (sort (:audit (:meta changed-info)))))))
          (println (.stringify js/JSON
            #js {:schema 1
                 :boundary "Pinned analyzer executing as CLJS; no Suss transport"
                 :windows rows
                 :identity #js {:fresh-info (not (identical? first-info second-info))
                                :first-meta (identical? saved (:meta first-info))
                                :second-meta (identical? saved (:meta second-info))
                                :audit (identical? audit (:audit (:meta second-info)))
                                :new-revision (not (identical? saved (:meta changed-info)))}
                 :values #js {:old-audit (into-array (sort audit))
                              :new-audit (into-array (sort (:audit (:meta changed-info))))}}))))
      (finally (set! cljs.core/hash original-hash)))))

(set! *main-cli-fn* -main)
