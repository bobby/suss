(ns suss-oracle.method-recurrence
  (:require [suss-oracle.declaration-macros :as declaration]))

;; Original projection of actual pinned fn-method facts; no reanalysis.
(defmacro observe [label binding]
  (let [methods (get-in &env [:locals binding :init :methods])
        row [label (mapv (fn [method]
                          [(contains? method :recurs) (:recurs method)
                           (contains? (:body method) :body?) (:body? (:body method))
                           (name (:context (:env method)))
                           (contains? (:locals (:env method)) 'x)]) methods)]]
    (spit "out/method-recurrence-calls.jsonl"
          (str (declaration/json row) "\n") :append true)
    (list 'quote row)))
