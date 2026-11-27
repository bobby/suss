(def metadata-identity-target :root)
(let [v #'metadata-identity-target
      original (meta v)
      same (assoc (assoc original :name (:name original)) :ns (:ns original))]
  (prn [(identical? original same)
        (identical? original (reset-meta! v same))
        (identical? original (meta v)) @v]))
