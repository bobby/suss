(ns suss-oracle.seqable)
(deftype SeqableProbe []
  cljs.core/ISeqable
  (-seq [_] (cljs.core/list 1)))
(defn -main []
  (println (.stringify js/JSON
    #js [#js {:id "nil" :value (cljs.core/seqable? nil)}
         #js {:id "vector" :value (cljs.core/seqable? [1])}
         #js {:id "list" :value (cljs.core/seqable? (cljs.core/list 1))}
         #js {:id "sorted-map" :value (cljs.core/seqable? (cljs.core/sorted-map 1 2))}
         #js {:id "sorted-set" :value (cljs.core/seqable? (cljs.core/sorted-set 1))}
         #js {:id "array" :value (cljs.core/seqable? (js/Array. 1))}
         #js {:id "string" :value (cljs.core/seqable? "x")}
         #js {:id "custom-ISeqable" :value (cljs.core/seqable? (SeqableProbe.))}
         #js {:id "number" :value (cljs.core/seqable? 1)}
         #js {:id "false" :value (cljs.core/seqable? false)}
         #js {:id "keyword" :value (cljs.core/seqable? :k)}
         #js {:id "plain-js-object" :value (cljs.core/seqable? (js-obj))}])))
(set! *main-cli-fn* -main)
