(ns suss-oracle.declaration-macros
  (:require [cljs.analyzer :as ana]
            [clojure.string :as str]))

;; Original development-only observation helper. No analyzer implementation is
;; copied here. Preserve field presence and data kinds, including nil versus an
;; empty sequence, rather than projecting every value to a printed string.
(def fields
  [:name :ns :tag :ret-tag :fn-var :variadic? :max-fixed-arity :method-params
   :arglists :arglists-meta :private :dynamic :doc :declared :line :column :file :meta])

(defn data [value]
  (cond
    (or (nil? value) (boolean? value) (string? value) (number? value)) value
    (symbol? value) ["symbol" (str value)]
    (keyword? value) ["keyword" (str value)]
    (vector? value) ["vector" (mapv data value)]
    (map? value) ["map" (mapv (fn [[k v]] [(data k) (data v)])
                              (sort-by (comp pr-str first) value))]
    (set? value) ["set" (mapv data (sort-by pr-str value))]
    (seq? value) ["seq" (mapv data value)]
    :else (throw (ex-info "Unsupported declaration observation" {:value value}))))

(defn json [value]
  (cond
    (nil? value) "null"
    (or (boolean? value) (number? value)) (str value)
    (string? value) (pr-str value)
    (vector? value) (str "[" (str/join "," (map json value)) "]")
    :else (throw (ex-info "Unsupported JSON projection" {:value value}))))

(defn declarations
  ([defs names] (declarations defs names fields))
  ([defs names selected-fields]
  (mapv (fn [sym]
          [(str sym) (contains? defs sym)
           (mapv (fn [key] [(name key) (contains? (get defs sym) key)
                           (data (get-in defs [sym key]))]) selected-fields)]) names)))

(defmacro observe [label names]
  (when-not (and (string? label) (vector? names) (every? symbol? names))
    (throw (ex-info "Expected a label and literal declaration symbols" {:form &form})))
  (let [ns (:ns &env)
        catalog (ana/get-namespace (:name ns))]
    (spit "out/declaration-environment-calls.jsonl"
          (str (json [label (str (:name ns))
                      (declarations (:defs ns) names)
                      (declarations (:defs catalog) names)]) "\n") :append true)
    42))

(defmacro observe-locals [label names]
  (when-not (and (string? label) (vector? names) (every? symbol? names))
    (throw (ex-info "Expected a label and literal local symbols" {:form &form})))
  (spit "out/declaration-local-calls.jsonl"
        (str (json [label (str (get-in &env [:ns :name]))
                    (declarations (:locals &env) names
                      [:name :local :tag :fn-var :variadic? :max-fixed-arity
                       :method-params :arglists])]) "\n") :append true)
  42)
