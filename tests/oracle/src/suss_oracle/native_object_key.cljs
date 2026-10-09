(ns suss-oracle.native-object-key)
(defn object-set [o k v] (js* "~{}[~{}] = ~{}" o k v))
(defn object-get [o k] (js* "~{}[~{}]" o k))
(defn -main []
  (let [o (js-obj) key (js-obj) result (object-set o key 7)
        trace (atom 0)
        hook-key (js-obj "toString" (fn [] (swap! trace #(+ (* % 10) 4)) "label")
                         "valueOf" (fn [] (reset! trace 999) (throw 91)))
        hook-result (object-set (do (reset! trace 1) o)
                                (do (swap! trace #(+ (* % 10) 2)) hook-key)
                                (do (swap! trace #(+ (* % 10) 3)) 23))
        hook-trace @trace
        throwing-key (js-obj "toString" (fn [] (swap! trace #(+ (* % 10) 4)) (throw 73))
                             "valueOf" (fn [] (reset! trace 999) 1))
        thrown (try (object-set (do (reset! trace 1) o)
                                (do (swap! trace #(+ (* % 10) 2)) throwing-key)
                                (do (swap! trace #(+ (* % 10) 3)) 99))
                    (catch :default e e))]
    (println (.stringify js/JSON #js {:plain-return result :plain-property (object-get o "[object Object]")
      :hook-return hook-result :hook-property (object-get o "label") :hook-trace hook-trace
      :thrown thrown :throw-trace @trace :preserved-property (object-get o "label")
      :recovery (object-get (js-obj "x" 19) "x")}))))
(set! *main-cli-fn* -main)
