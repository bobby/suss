(assert (= "1.12.6" (clojure-version)))
(prn (let [f (with-meta (fn [n] n) {:marker 1})] [(fn? f) (identical? f (eval f)) ((eval f) 42) (meta (eval f))]))
(prn (do (def review-meta-effects (atom [])) (let [f (with-meta (fn [n] n) {:marker 1})] (try (eval (list 'do '(swap! review-meta-effects conj :prefix) (list 'identity f))) (catch Throwable e nil))) @review-meta-effects))
(prn (let [x 1 f (fn [] x)] [(identical? f (eval f)) ((eval f))]))
