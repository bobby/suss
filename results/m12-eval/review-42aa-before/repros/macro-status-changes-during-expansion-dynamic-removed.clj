(assert (= "1.12.6" (clojure-version)))
(prn (do (declare changing-macro) (eval '(defmacro ^:dynamic changing-macro [] 1)) (let [f (fn [& xs] (if (empty? xs) 43 (do (eval '(def changing-macro (fn [] 45))) 42)))] (binding [changing-macro f] (eval '[(changing-macro) (changing-macro)])))))
