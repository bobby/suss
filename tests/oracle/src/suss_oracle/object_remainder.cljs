(ns suss-oracle.object-remainder)
(defn -main []
  (let [trace (atom 0)
        payload (try
                  (js-mod (do (reset! trace 1)
                              (js-obj "valueOf" (fn [] (swap! trace #(+ (* % 10) 3)) (throw 73))))
                          (do (swap! trace #(+ (* % 10) 2))
                              (js-obj "valueOf" (fn [] (reset! trace 999) 3))))
                  (catch :default e e))]
    (println (.stringify js/JSON
      #js [#js {:id "object-remainder-nan" :value (js/isNaN (js-mod (js-obj) 3))}
           #js {:id "object-remainder-conversion-throw" :value (and (= payload 73) (= @trace 123))}
           #js {:id "object-remainder-recovery" :value (= (js-mod 7 3) 1)}]))))
(set! *main-cli-fn* -main)
