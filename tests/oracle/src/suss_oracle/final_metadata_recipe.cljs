(ns suss-oracle.final-metadata-recipe
  (:require [cljs.analyzer :as ana]
            [cljs.env :as env]
            [cljs.js :as cljs]
            [cljs.tools.reader :as reader]
            [suss-oracle.main :as transport]))

;; Original development-only observations. Analyzer paths execute as CLJS;
;; runtime metadata replacement is a separate, explicit portable boundary.
(def probe-ns 'suss-oracle.final-metadata)
(defn analyze [state source]
  (binding [env/*compiler* state
            ana/*cljs-ns* probe-ns
            ana/*analyze-deps* false
            ana/*load-macros* false]
    (ana/analyze (assoc (ana/empty-env) :ns (ana/get-namespace probe-ns))
      (reader/read-string source))))
(defn -main []
  (let [state (cljs/empty-state)]
    (swap! state assoc-in [::ana/namespaces probe-ns] {:name probe-ns :defs {}})
    (println (.stringify js/JSON
      #js {:schema 2
           :upstream "c4295f303100bbf5afac449242d30bca1126f1a1"
           :cases
           (into-array
             (concat
               (map (fn [[id source]]
                      (transport/observation id
                        (fn [_]
                          (analyze state source)
                          (let [info (get-in @state [::ana/namespaces probe-ns :defs 'watched])
                                metadata (:meta info)]
                            (assert (map? info) "Expected declaration info")
                            [(contains? info :meta) (nil? metadata) (map? metadata)
                             (count metadata) metadata (meta metadata)]))))
                 [["top-fn-custom-meta" "(def ^{:top-fn {:meta {:custom true}}} watched (fn* [] 17))"]
                  ["top-fn-nil-meta" "(def ^{:top-fn {:meta nil}} watched (fn* [] 17))"]
                  ["top-fn-empty-meta" "(def ^{:top-fn {:meta {}}} watched (fn* [] 17))"]])
               [(transport/observation "metadata-map-retains-own-metadata"
                  (fn [_]
                    (let [metadata (with-meta {:m 17} {:on-meta true})
                          base (with-meta [1 2] {:old true})
                          value (with-meta base metadata)]
                      [(identical? metadata (meta value)) (meta value)
                       (meta (meta value)) (meta base) value])))]))}))))
(set! *main-cli-fn* -main)
