(assert (= "1.12.6" (clojure-version)))
(prn [(do (eval '(defmacro publication-target [] 1)) (eval '(defmacro publication-reassert [] (eval '(defmacro publication-target [] 42)) 0)) [(load-string "(defmacro publication-target [] 17) (def publication-answer [(publication-reassert) (publication-target)]) publication-answer") (eval '(publication-target))])
(do (try (load-string "(eval '(def script-prefix-saved 21)) (throw (ex-info \"stop\" {})) 0") (catch Throwable _ :caught)) (eval 'script-prefix-saved))])
