(ns suss-oracle.queue-iterator)
(def q (conj (conj (conj (.-EMPTY PersistentQueue) 1) 2) 3))
(def qi (-iterator q))
(defn -main []
 (println (.stringify js/JSON (into-array
  [(identical? (.hasNext qi) (.-front q)) (boolean (.hasNext qi)) (.next qi) (.hasNext qi) (.next qi) (.hasNext qi) (.next qi) (.hasNext qi)
   (try (.next qi) "unexpected-success" (catch :default e #js {:error (instance? js/Error e) :message (.-message e)}))
   (let [e (.remove qi)] #js {:error (instance? js/Error e) :message (.-message e)})
   (= (seq q) [1 2 3])]))))
(set! *main-cli-fn* -main)
