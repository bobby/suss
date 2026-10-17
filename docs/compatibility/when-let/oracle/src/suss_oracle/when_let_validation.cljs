(ns suss-oracle.when-let-validation
  (:require [cljs.js :as cljs]))

;; Development only: cljs.js loads the actual pinned self-host macro module.
;; Invoke that compiled macro function, not a substitute reader/evaluator.
(defn -main []
  (let [macro-function (aget (js* "cljs.core$macros") "when_let")
        forms ['(when-let 17 1) '(when-let [] 1) '(when-let [x] 1)
               '(when-let [x 1 y 2] x) '(when-let (x 1) 1)]
        rows (array)]
    (assert (fn? macro-function))
    (doseq [form forms]
      (try
        (let [expanded (apply macro-function form nil (rest form))]
          (.push rows #js {:source (pr-str form) :status "expanded"
                          :expansion (pr-str expanded)}))
        (catch :default error
          (.push rows #js {:source (pr-str form) :status "exception"
                          :message (ex-message error)
                          :phase (str (:clojure.error/phase (ex-data error)))
                          :data (pr-str (ex-data error))}))))
    (let [valid '(when-let [x 17] x)
          recovered (apply macro-function valid nil (rest valid))]
      (println (.stringify js/JSON
        #js {:schema 1 :upstream "c4295f303100bbf5afac449242d30bca1126f1a1"
             :scope "Actual compiled self-host core macro function; diagnostics exclude analyzer wrapping."
             :cases rows :recovery-expansion (pr-str recovered)})))))
(set! *main-cli-fn* -main)
