(defmacro catalog-only [] 42)
(prn [(let [m (ns-interns 'user) v (get m 'catalog-only)]
   [(var? v) (if v (.isMacro v) false) (if v (fn? @v) false)
    (if v ((deref v) nil nil) -1)
    (identical? v (get (ns-interns 'user) 'catalog-only))])
 (let [m (ns-interns (:ns (meta #'identity))) v (get m 'when-let)]
   [(var? v) (if v (.isMacro v) false) (if v (fn? @v) false)
    (identical? v (get (ns-interns (:ns (meta #'identity))) 'when-let))])]
)
