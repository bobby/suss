(defn- create-tree-map-seq [tree ascending? cnt]
  (PersistentTreeMapSeq. nil (tree-map-seq-push tree nil ascending?) ascending? cnt nil))
