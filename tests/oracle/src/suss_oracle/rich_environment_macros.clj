(ns suss-oracle.rich-environment-macros
  (:require [clojure.string :as str]))

(spit "out/rich-environment-calls.jsonl" "")

;; Original development-only projection of the real pinned analyzer data.
;; Reject unrecognized values instead of pretending to serialize JVM objects.
(defn json [value]
  (cond
    (nil? value) "null"
    (string? value) (pr-str value)
    (boolean? value) (str value)
    (and (integer? value) (<= 0 value)) (str value)
    (vector? value) (str "[" (str/join "," (map json value)) "]")
    :else (throw (ex-info "Unexpected rich environment projection" {:value value}))))

(defn tag [value]
  (cond
    (nil? value) nil
    (symbol? value) (str value)
    (set? value) (mapv str (sort-by str value))
    :else (throw (ex-info "Unexpected analyzer tag" {:value value}))))

(declare binding-fact)
(def ^:dynamic *private-catch-names* #{})
(defn source-name [value]
  (when value
    (if (contains? *private-catch-names* value) "<private-catch>" (str value))))
(defn ast-fact [ast]
  (when ast
    [(some-> (:op ast) name) (tag (:tag ast))
     (when-let [form (:form ast)]
       (if (and (= :local (:op ast)) (contains? *private-catch-names* form))
         "<private-catch>" (pr-str form)))
     (mapv name (:children ast))
     (some-> ast :env :context name)]))

(defn binding-fact [binding]
  (when binding
    [(some-> (:op binding) name)
     (source-name (:name binding))
     (some-> (:local binding) name)
     (:arg-id binding) (:variadic? binding)
     (tag (:tag binding))
     (:binding-form? binding) (:field binding)
     (:mutable binding) (:unsynchronized-mutable binding) (:volatile-mutable binding)
     (:line binding) (:column binding)
     (some-> binding :env :context name)
     (ast-fact (:init binding))
     (binding-fact (:shadow binding))]))

(defmacro environment [label]
  ;; Normalize only compiler-owned catch names, identified by their actual role.
  ;; User source names and all other facts remain untouched.
  (binding [*private-catch-names* (set (keep (fn [[name binding]]
                                            (when (= :catch (:local binding)) name))
                                          (:locals &env)))]
  (let [locals (mapv (fn [[name binding]] [(source-name name) (binding-fact binding)])
                    (sort-by (comp source-name key) (:locals &env)))
        scopes (mapv (fn [scope]
                       [(str (:name scope)) (some-> scope :local name)
                        (boolean (get-in scope [:info :fn-self-name]))
                        (mapv #(str (:name %)) (get-in scope [:info :fn-scope]))
                        (binding-fact (get-in scope [:info :shadow]))])
                     (:fn-scope &env))]
    (spit "out/rich-environment-calls.jsonl"
          (str (json [label (name (:context &env)) (str (get-in &env [:ns :name]))
                      locals scopes]) "\n") :append true)
    (count (:locals &env)))))
