(ns suss-oracle.array-inc-dec)
;; Original reference cases for retained first-class source inc/dec.
(defn -main [] (println (.stringify js/JSON
  #js [#js {:id "inc-array-default-string" :value (let [f inc] (= (f (array 1)) "11"))}
       #js {:id "dec-array-number" :value (let [f dec] (= (f (array 1)) 0))}])))
(set! *main-cli-fn* -main)
