(ns suss-oracle.sorted-tree-sharing)
(defn raw-node [node]
  (if (nil? node)
    nil
    #js {:class (cond (instance? cljs.core/BlackNode node) "BlackNode"
                          (instance? cljs.core/RedNode node) "RedNode"
                          :else (throw (js/Error. "Unknown node class")))
         :key (.-key node) :val (.-val node)
         :left (raw-node (.-left node)) :right (raw-node (.-right node))
         :hash (.-__hash node)}))
(defn -main []
  (let [old (sorted-map 2 20 1 10 3 30)
        updated (assoc old 4 40)
        old-root (.-tree old) new-root (.-tree updated)]
    (println (.stringify js/JSON
      #js {:old (raw-node old-root) :updated (raw-node new-root)
           :rootChanged (not (identical? old-root new-root))
           :directLeftShared (identical? (.-left old-root) (.-left new-root))
           :nestedLeftShared (identical? (.-left old-root) (.-left (.-left new-root)))
           :comparatorShared (identical? (.-comp old) (.-comp updated))}))))
(set! *main-cli-fn* -main)
