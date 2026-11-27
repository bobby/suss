(prn (let [n (:ns (meta #'identity)) m (ns-interns n)] [(var? (get m 'identity)) (identical? #'identity (get m 'identity)) (identical? n (:ns (meta (get m 'identity))))]))
