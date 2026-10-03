(ns graph.tools (:require [clojure.string :as str]))
(defmacro one [] 41)
(defmacro two [] 42)
;; Original development-only projection of actual pinned analyzer namespace maps.
(defn json [value]
  (cond
    (nil? value) "null"
    (boolean? value) (str value)
    (string? value) (pr-str value)
    (vector? value) (str "[" (str/join "," (map json value)) "]")
    :else (throw (ex-info "Unexpected namespace projection" {:value value}))))
(defmacro observe []
  (let [ns (:ns &env)
        keys [:requires :uses :renames :require-macros :use-macros :rename-macros]
        maps (mapv (fn [key]
                     [(name key) (contains? ns key)
                      (when-let [entries (get ns key)]
                        (when-not (map? entries)
                          (throw (ex-info "Unexpected namespace map" {:key key :value entries})))
                        (mapv (fn [[key value]]
                                (when-not (and (symbol? key) (symbol? value))
                                  (throw (ex-info "Unexpected namespace entry" {:key key :value value})))
                                [(str key) (str value)])
                              (sort-by (comp str first) entries)))]) keys)]
    (spit "out/namespace-environment-calls.jsonl"
          (str "{\"schema\":1,\"upstream\":\"c4295f303100bbf5afac449242d30bca1126f1a1\",\"namespace\":"
               (json (str (:name ns))) ",\"maps\":" (json maps) "}\n") :append true)
    42))
