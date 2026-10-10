(core/defn- annotate-specs [annots v [f sigs]]
  (conj v
    (vary-meta (cons f (map #(cons (second %) (nnext %)) sigs))
      merge annots)))
