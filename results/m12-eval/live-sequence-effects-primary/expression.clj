(defmacro source-lazy-probe [f]
 (let [state (atom 0)
       xs (lazy-seq (swap! state inc) (list 42))
       read-state (fn [] @state)]
  (f xs read-state)))

(prn [(eval (list 'user/source-lazy-probe (fn [xs read-state] [(read-state) (first xs) (read-state) (first xs) (read-state)])))
 (let [observer (eval (list 'user/source-lazy-probe (fn [xs read-state] (fn [] [(read-state) (first xs) (read-state) (first xs) (read-state)]))))] [(observer) (observer)])])
