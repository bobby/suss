(defn elide-reader-meta [m]
  (dissoc m :file :line :column :end-column :end-line :source))
