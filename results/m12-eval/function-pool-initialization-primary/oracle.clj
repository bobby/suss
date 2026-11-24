(assert (= "1.12.6" (clojure-version)))
(prn (do (def review-inner-effects (atom [])) (let [x 1 f (fn [] x)] (try (eval (list 'let [] (list 'do '(swap! review-inner-effects conj :prefix) (list 'fn [] (list 'quote f))))) (catch Throwable e nil))) @review-inner-effects))
(prn (let [x 1 f (fn [] x)] (try (eval (list 'if false (list 'fn [] (list 'quote f)) :safe)) (catch Throwable e :error))))
(prn (let [f (fn [n] n) g (eval (list 'fn (list [] (list 'quote f)) (list ['x] (list 'quote f))))] [(identical? f (g)) (identical? (g) (g 1))]))
